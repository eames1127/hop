//! Background `ssh -N -L` tunnels started from a host's saved port forwards.
//!
//! Each tunnel is a plain `ssh` child process with no terminal. Hop keeps the handles so it can
//! stop them on request, notice when one dies (port in use, authentication refused) and kill
//! them all when it quits.

use std::io::{self, Read};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde::Serialize;
use sshconf::{is_valid_host_name, Forward};

/// Longest stderr kept per tunnel; ssh only prints a few lines before it gives up.
const MAX_STDERR_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TunnelState {
	Running,
	Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelView {
	pub host: String,
	/// Position of the forward in the host's saved list.
	pub forward: usize,
	pub local_port: u16,
	pub remote_host: String,
	pub remote_port: u16,
	pub label: String,
	pub state: TunnelState,
	pub error: Option<String>,
}

/// A running tunnel process. A trait so the bookkeeping can be tested without starting `ssh`.
pub trait TunnelProcess: Send {
	/// `None` while it runs; once it has exited, whatever it wrote to stderr.
	fn exited(&mut self) -> Option<String>;
	fn stop(&mut self);
}

/// The arguments for `ssh`, built from checked parts and never passed through a shell.
///
/// `BatchMode` makes ssh fail at once instead of waiting for a password prompt that nobody
/// can see, and `ExitOnForwardFailure` makes a busy local port an error rather than a tunnel
/// that silently forwards nothing.
pub fn tunnel_args(name: &str, forward: &Forward) -> Result<Vec<String>, String> {
	if !is_valid_host_name(name) {
		return Err(format!("\"{name}\" is not a valid host name"));
	}
	if !forward.is_valid() {
		return Err("That port forward is not valid".to_string());
	}

	Ok(vec![
		"-N".to_string(),
		"-o".to_string(),
		"BatchMode=yes".to_string(),
		"-o".to_string(),
		"ExitOnForwardFailure=yes".to_string(),
		"-L".to_string(),
		forward.spec(),
		name.to_string(),
	])
}

/// Turns ssh's last words into something a person can act on.
pub fn explain_failure(stderr: &str, local_port: u16) -> String {
	let lower = stderr.to_lowercase();
	if lower.contains("address already in use")
		|| lower.contains("cannot listen to port")
		|| lower.contains("could not request local forwarding")
	{
		format!("localhost:{local_port} is already in use by another program.")
	}
	else if lower.contains("host key verification failed") {
		"The host key is not trusted yet. Use Connect once to check and accept it.".to_string()
	}
	else if lower.contains("permission denied") {
		"The host asked for a password or passphrase, which a background tunnel cannot type. \
		 Set up key authentication (with ssh-agent), or use Connect for this forward."
			.to_string()
	}
	else {
		match stderr
			.lines()
			.rev()
			.map(str::trim)
			.find(|line| !line.is_empty())
		{
			Some(line) => format!("ssh stopped: {line}"),
			None => "ssh stopped without saying why.".to_string(),
		}
	}
}

struct SshProcess {
	child: Child,
	stderr: Arc<Mutex<String>>,
	reader: Option<JoinHandle<()>>,
}

impl SshProcess {
	fn captured_stderr(&mut self) -> String {
		// ssh has exited, so its stderr reaches end of file almost at once; wait briefly so
		// the explanation is not lost, but never hang if a jump host child kept the pipe open.
		if let Some(reader) = self.reader.take() {
			for _ in 0..20 {
				if reader.is_finished() {
					break;
				}
				thread::sleep(Duration::from_millis(10));
			}
			if reader.is_finished() && reader.join().is_err() {
				eprintln!("hop: the tunnel output reader panicked");
			}
		}

		match self.stderr.lock() {
			Ok(text) => text.clone(),
			Err(_) => String::new(),
		}
	}
}

impl TunnelProcess for SshProcess {
	fn exited(&mut self) -> Option<String> {
		match self.child.try_wait() {
			Ok(None) => None,
			Ok(Some(_)) => Some(self.captured_stderr()),
			Err(error) => {
				self.stop();
				Some(format!("could not check the tunnel: {error}"))
			}
		}
	}

	fn stop(&mut self) {
		if let Err(error) = self.child.kill() {
			// Already exited is the usual reason, and is fine.
			if error.kind() != io::ErrorKind::InvalidInput {
				eprintln!("hop: could not stop a tunnel: {error}");
			}
		}
		if let Err(error) = self.child.wait() {
			eprintln!("hop: could not reap a tunnel: {error}");
		}
	}
}

/// Starts `ssh` with no window and no input, keeping its stderr for error messages.
pub fn spawn_ssh(ssh: &Path, args: &[String]) -> io::Result<Box<dyn TunnelProcess>> {
	let mut command = Command::new(ssh);
	command
		.args(args)
		.stdin(Stdio::null())
		.stdout(Stdio::null())
		.stderr(Stdio::piped());
	hide_console(&mut command);
	let mut child = command.spawn()?;

	let stderr = Arc::new(Mutex::new(String::new()));
	let reader = child.stderr.take().map(|mut pipe| {
		let stderr = Arc::clone(&stderr);
		thread::spawn(move || {
			let mut buffer = [0_u8; 1024];
			loop {
				match pipe.read(&mut buffer) {
					Ok(0) | Err(_) => break,
					Ok(count) => {
						let Ok(mut text) = stderr.lock()
						else {
							break;
						};
						if text.len() < MAX_STDERR_BYTES {
							text.push_str(&String::from_utf8_lossy(&buffer[..count]));
						}
					}
				}
			}
		})
	});

	Ok(Box::new(SshProcess {
		child,
		stderr,
		reader,
	}))
}

#[cfg(windows)]
fn hide_console(command: &mut Command) {
	use std::os::windows::process::CommandExt;

	const CREATE_NO_WINDOW: u32 = 0x0800_0000;
	command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console(_command: &mut Command) {}

struct Entry {
	view: TunnelView,
	process: Option<Box<dyn TunnelProcess>>,
}

/// Every tunnel Hop started, keyed by local port because only one program can listen on it.
/// Failed tunnels stay listed with their error until they are restarted or dismissed.
#[derive(Default)]
pub struct Tunnels {
	entries: Mutex<Vec<Entry>>,
}

impl Tunnels {
	fn entries(&self) -> Result<MutexGuard<'_, Vec<Entry>>, String> {
		self.entries
			.lock()
			.map_err(|_| "Tunnels are unavailable after an earlier error".to_string())
	}

	pub fn start<F>(
		&self,
		name: &str,
		index: usize,
		forward: &Forward,
		spawn: F,
	) -> Result<(), String>
	where
		F: FnOnce(&[String]) -> io::Result<Box<dyn TunnelProcess>>,
	{
		let args = tunnel_args(name, forward)?;
		let mut entries = self.entries()?;
		let busy = entries.iter().find(|entry| {
			entry.view.local_port == forward.local_port && entry.view.state == TunnelState::Running
		});
		if let Some(entry) = busy {
			return Err(format!(
				"localhost:{} is already forwarded to {}",
				forward.local_port, entry.view.host
			));
		}

		let process = spawn(&args).map_err(|error| format!("Could not start ssh: {error}"))?;
		entries.retain(|entry| {
			entry.view.local_port != forward.local_port
				&& !(entry.view.host == name && entry.view.forward == index)
		});
		entries.push(Entry {
			view: TunnelView {
				host: name.to_string(),
				forward: index,
				local_port: forward.local_port,
				remote_host: forward.remote_host.clone(),
				remote_port: forward.remote_port,
				label: forward.label.clone(),
				state: TunnelState::Running,
				error: None,
			},
			process: Some(process),
		});
		Ok(())
	}

	/// Lists a tunnel that could not be started as failed, so a problem found while nobody was
	/// watching (at launch) is still shown on the host card and in the tray afterwards.
	pub fn record_failure(
		&self,
		name: &str,
		index: usize,
		forward: &Forward,
		error: String,
	) -> Result<(), String> {
		let mut entries = self.entries()?;
		if entries
			.iter()
			.any(|entry| entry.view.local_port == forward.local_port)
		{
			// Whatever already holds the port is more useful to show than this failure.
			return Ok(());
		}

		entries.push(Entry {
			view: TunnelView {
				host: name.to_string(),
				forward: index,
				local_port: forward.local_port,
				remote_host: forward.remote_host.clone(),
				remote_port: forward.remote_port,
				label: forward.label.clone(),
				state: TunnelState::Failed,
				error: Some(error),
			},
			process: None,
		});
		Ok(())
	}

	/// Stops the tunnel on `local_port`, or forgets it if it had already failed.
	pub fn stop(&self, local_port: u16) -> Result<(), String> {
		let mut entries = self.entries()?;
		if let Some(position) = entries
			.iter()
			.position(|entry| entry.view.local_port == local_port)
		{
			let mut entry = entries.remove(position);
			if let Some(process) = entry.process.as_mut() {
				process.stop();
			}
		}
		Ok(())
	}

	pub fn list(&self) -> Result<Vec<TunnelView>, String> {
		Ok(self
			.entries()?
			.iter()
			.map(|entry| entry.view.clone())
			.collect())
	}

	/// Marks tunnels whose ssh has exited as failed. Returns whether anything changed.
	pub fn poll(&self) -> Result<bool, String> {
		let mut changed = false;
		for entry in self.entries()?.iter_mut() {
			let Some(process) = entry.process.as_mut()
			else {
				continue;
			};
			if let Some(stderr) = process.exited() {
				entry.process = None;
				entry.view.state = TunnelState::Failed;
				entry.view.error = Some(explain_failure(&stderr, entry.view.local_port));
				changed = true;
			}
		}
		Ok(changed)
	}

	pub fn stop_all(&self) {
		// Even after a panic elsewhere the children must not outlive Hop.
		let mut entries = match self.entries.lock() {
			Ok(entries) => entries,
			Err(poisoned) => poisoned.into_inner(),
		};
		for entry in entries.iter_mut() {
			if let Some(process) = entry.process.as_mut() {
				process.stop();
			}
		}
		entries.clear();
	}
}

impl Drop for Tunnels {
	fn drop(&mut self) {
		self.stop_all();
	}
}

#[cfg(test)]
mod tests {
	use std::sync::atomic::{AtomicBool, Ordering};

	use super::*;

	struct FakeProcess {
		exit: Arc<Mutex<Option<String>>>,
		stopped: Arc<AtomicBool>,
	}

	impl TunnelProcess for FakeProcess {
		fn exited(&mut self) -> Option<String> {
			self.exit.lock().unwrap().clone()
		}

		fn stop(&mut self) {
			self.stopped.store(true, Ordering::SeqCst);
		}
	}

	struct Handle {
		exit: Arc<Mutex<Option<String>>>,
		stopped: Arc<AtomicBool>,
	}

	impl Handle {
		fn new() -> Handle {
			Handle {
				exit: Arc::new(Mutex::new(None)),
				stopped: Arc::new(AtomicBool::new(false)),
			}
		}

		fn spawner(&self) -> impl FnOnce(&[String]) -> io::Result<Box<dyn TunnelProcess>> {
			let exit = Arc::clone(&self.exit);
			let stopped = Arc::clone(&self.stopped);
			move |_| Ok(Box::new(FakeProcess { exit, stopped }) as Box<dyn TunnelProcess>)
		}
	}

	fn forward(local_port: u16) -> Forward {
		Forward {
			local_port,
			remote_host: "localhost".to_string(),
			remote_port: 5001,
			label: "Admin UI".to_string(),
			auto_start: false,
		}
	}

	#[test]
	fn arguments_are_a_fixed_list_ending_in_the_alias() {
		let args = tunnel_args("nas", &forward(8080)).unwrap();
		assert_eq!(
			args,
			[
				"-N",
				"-o",
				"BatchMode=yes",
				"-o",
				"ExitOnForwardFailure=yes",
				"-L",
				"8080:localhost:5001",
				"nas"
			]
		);
	}

	#[test]
	fn bad_names_and_forwards_are_refused() {
		assert!(tunnel_args("nas; calc", &forward(8080)).is_err());
		assert!(tunnel_args("-oProxyCommand=calc", &forward(8080)).is_err());
		let mut bad = forward(8080);
		bad.remote_host = "evil host".to_string();
		assert!(tunnel_args("nas", &bad).is_err());
	}

	#[test]
	fn start_list_and_stop() {
		let tunnels = Tunnels::default();
		let handle = Handle::new();
		tunnels
			.start("nas", 0, &forward(8080), handle.spawner())
			.unwrap();

		let list = tunnels.list().unwrap();
		assert_eq!(list.len(), 1);
		assert_eq!(list[0].host, "nas");
		assert_eq!(list[0].state, TunnelState::Running);

		tunnels.stop(8080).unwrap();
		assert!(handle.stopped.load(Ordering::SeqCst));
		assert!(tunnels.list().unwrap().is_empty());
	}

	#[test]
	fn a_busy_local_port_is_refused() {
		let tunnels = Tunnels::default();
		let first = Handle::new();
		let second = Handle::new();
		tunnels
			.start("nas", 0, &forward(8080), first.spawner())
			.unwrap();
		let error = tunnels
			.start("web", 0, &forward(8080), second.spawner())
			.unwrap_err();
		assert!(error.contains("nas"));
	}

	#[test]
	fn early_exit_is_reported_and_can_be_retried() {
		let tunnels = Tunnels::default();
		let handle = Handle::new();
		tunnels
			.start("nas", 0, &forward(8080), handle.spawner())
			.unwrap();
		assert!(!tunnels.poll().unwrap());

		*handle.exit.lock().unwrap() = Some("me@nas: Permission denied (publickey).\n".into());
		assert!(tunnels.poll().unwrap());
		let list = tunnels.list().unwrap();
		assert_eq!(list[0].state, TunnelState::Failed);
		assert!(list[0]
			.error
			.as_deref()
			.unwrap()
			.contains("key authentication"));
		assert!(!tunnels.poll().unwrap());

		let retry = Handle::new();
		tunnels
			.start("nas", 0, &forward(8080), retry.spawner())
			.unwrap();
		let list = tunnels.list().unwrap();
		assert_eq!(list.len(), 1);
		assert_eq!(list[0].state, TunnelState::Running);
	}

	#[test]
	fn recorded_failures_are_listed_but_never_hide_a_running_tunnel() {
		let tunnels = Tunnels::default();
		tunnels
			.record_failure("nas", 0, &forward(8080), "Could not start ssh".to_string())
			.unwrap();
		let list = tunnels.list().unwrap();
		assert_eq!(list[0].state, TunnelState::Failed);
		assert_eq!(list[0].error.as_deref(), Some("Could not start ssh"));

		let handle = Handle::new();
		tunnels
			.start("nas", 0, &forward(8080), handle.spawner())
			.unwrap();
		tunnels
			.record_failure("web", 0, &forward(8080), "busy".to_string())
			.unwrap();
		let list = tunnels.list().unwrap();
		assert_eq!(list.len(), 1);
		assert_eq!(list[0].state, TunnelState::Running);
	}

	#[test]
	fn stop_all_stops_every_tunnel() {
		let tunnels = Tunnels::default();
		let first = Handle::new();
		let second = Handle::new();
		tunnels
			.start("nas", 0, &forward(8080), first.spawner())
			.unwrap();
		tunnels
			.start("web", 1, &forward(9090), second.spawner())
			.unwrap();
		tunnels.stop_all();
		assert!(first.stopped.load(Ordering::SeqCst));
		assert!(second.stopped.load(Ordering::SeqCst));
		assert!(tunnels.list().unwrap().is_empty());
	}

	#[test]
	fn failures_are_explained() {
		let busy = "bind [127.0.0.1]:8080: Address already in use\n\
		            Could not request local forwarding.";
		assert!(explain_failure(busy, 8080).contains("localhost:8080 is already in use"));
		assert!(explain_failure("Host key verification failed.", 1).contains("Connect"));
		assert_eq!(explain_failure("", 1), "ssh stopped without saying why.");
		assert_eq!(
			explain_failure("\nssh: Could not resolve hostname nas\n\n", 1),
			"ssh stopped: ssh: Could not resolve hostname nas"
		);
	}
}
