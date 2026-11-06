use std::io;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::reap;

pub const TERMINALS: &[(&str, &str)] = &[
	("auto", "Automatic (Windows Terminal, then PowerShell)"),
	("windows-terminal", "Windows Terminal"),
	("powershell", "PowerShell window"),
];

const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

fn system_root() -> PathBuf {
	std::env::var_os("SystemRoot")
		.map(PathBuf::from)
		.unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
}

/// Windows' built-in OpenSSH, falling back to whatever `ssh` is on the PATH.
pub fn ssh_program() -> PathBuf {
	let built_in = system_root().join(r"System32\OpenSSH\ssh.exe");
	if built_in.exists() {
		built_in
	}
	else {
		PathBuf::from("ssh")
	}
}

fn windows_terminal() -> Option<PathBuf> {
	let local = std::env::var_os("LOCALAPPDATA")?;
	let path = PathBuf::from(local).join(r"Microsoft\WindowsApps\wt.exe");
	path.exists().then_some(path)
}

/// Opens `ssh <args>` in a new terminal. `ssh_args` are built and checked by Hop, never taken
/// from the page as raw text.
pub fn launch_terminal(terminal: &str, title: &str, ssh_args: &[String]) -> io::Result<()> {
	match terminal {
		"powershell" => launch_powershell(ssh_args),
		"windows-terminal" => match windows_terminal() {
			Some(wt) => launch_windows_terminal(&wt, title, ssh_args),
			None => Err(io::Error::new(
				io::ErrorKind::NotFound,
				"Windows Terminal (wt.exe) is not installed",
			)),
		},
		_ => match windows_terminal() {
			Some(wt) => launch_windows_terminal(&wt, title, ssh_args),
			None => launch_powershell(ssh_args),
		},
	}
}

fn launch_windows_terminal(wt: &Path, title: &str, ssh_args: &[String]) -> io::Result<()> {
	let child = Command::new(wt)
		.arg("new-tab")
		.arg("--title")
		.arg(title)
		.arg(ssh_program())
		.args(ssh_args)
		.spawn()?;
	reap(child);
	Ok(())
}

/// PowerShell takes one command string, so the arguments are joined. They only ever contain
/// `[A-Za-z0-9._:-]`, which PowerShell does not treat specially.
fn launch_powershell(ssh_args: &[String]) -> io::Result<()> {
	let script = format!("& '{}' {}", ssh_program().display(), ssh_args.join(" "));
	let child = Command::new(system_root().join(r"System32\WindowsPowerShell\v1.0\powershell.exe"))
		.arg("-NoExit")
		.arg("-Command")
		.arg(script)
		.creation_flags(CREATE_NEW_CONSOLE)
		.spawn()?;
	reap(child);
	Ok(())
}
