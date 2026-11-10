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
	open_ssh_program("ssh")
}

pub fn ssh_keygen_program() -> PathBuf {
	open_ssh_program("ssh-keygen")
}

fn open_ssh_program(name: &str) -> PathBuf {
	let built_in = system_root()
		.join(r"System32\OpenSSH")
		.join(format!("{name}.exe"));
	if built_in.exists() {
		built_in
	}
	else {
		PathBuf::from(name)
	}
}

fn windows_terminal() -> Option<PathBuf> {
	let local = std::env::var_os("LOCALAPPDATA")?;
	let path = PathBuf::from(local).join(r"Microsoft\WindowsApps\wt.exe");
	path.exists().then_some(path)
}

/// Opens `<program> <args>` in a new terminal. `args` are built and checked by Hop, never taken
/// from the page as raw text.
pub fn launch_terminal(
	terminal: &str,
	title: &str,
	program: &Path,
	args: &[String],
) -> io::Result<()> {
	match terminal {
		"powershell" => launch_powershell(program, args),
		"windows-terminal" => match windows_terminal() {
			Some(wt) => launch_windows_terminal(&wt, title, program, args),
			None => Err(io::Error::new(
				io::ErrorKind::NotFound,
				"Windows Terminal (wt.exe) is not installed",
			)),
		},
		_ => match windows_terminal() {
			Some(wt) => launch_windows_terminal(&wt, title, program, args),
			None => launch_powershell(program, args),
		},
	}
}

fn launch_windows_terminal(
	wt: &Path,
	title: &str,
	program: &Path,
	args: &[String],
) -> io::Result<()> {
	let child = Command::new(wt)
		.arg("new-tab")
		.arg("--title")
		.arg(title)
		.arg(program)
		.args(args)
		.spawn()?;
	reap(child);
	Ok(())
}

/// Single quotes make PowerShell take the text literally; a quote inside is written twice.
fn powershell_quote(text: &str) -> String {
	format!("'{}'", text.replace('\'', "''"))
}

/// PowerShell takes one command string, so the program and arguments are quoted and joined.
/// Key paths sit under the user's profile folder, which may contain spaces or quotes.
fn launch_powershell(program: &Path, args: &[String]) -> io::Result<()> {
	let quoted: Vec<String> = args.iter().map(|arg| powershell_quote(arg)).collect();
	let script = format!(
		"& {} {}",
		powershell_quote(&program.to_string_lossy()),
		quoted.join(" ")
	);
	let child = Command::new(system_root().join(r"System32\WindowsPowerShell\v1.0\powershell.exe"))
		.arg("-NoExit")
		.arg("-Command")
		.arg(script)
		.creation_flags(CREATE_NEW_CONSOLE)
		.spawn()?;
	reap(child);
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn powershell_quoting_keeps_text_literal() {
		assert_eq!(powershell_quote("nas"), "'nas'");
		assert_eq!(
			powershell_quote(r"C:\Users\Ann O'Neil\.ssh\id_ed25519"),
			r"'C:\Users\Ann O''Neil\.ssh\id_ed25519'"
		);
		assert_eq!(powershell_quote("$(calc)"), "'$(calc)'");
	}
}
