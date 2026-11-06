use std::env;
use std::io;
use std::path::PathBuf;
use std::process::Command;

use super::reap;

pub const TERMINALS: &[(&str, &str)] = &[
	("auto", "Automatic (Konsole if installed)"),
	("konsole", "Konsole"),
	("kitty", "Kitty"),
	("xterm", "XTerm"),
];

pub fn ssh_program() -> PathBuf {
	find_on_path("ssh").unwrap_or_else(|| PathBuf::from("ssh"))
}

fn find_on_path(program: &str) -> Option<PathBuf> {
	let paths = env::var_os("PATH")?;
	env::split_paths(&paths)
		.map(|dir| dir.join(program))
		.find(|path| path.is_file())
}

/// Opens `ssh <args>` in a new terminal. `ssh_args` are built and checked by Hop, never taken
/// from the page as raw text.
pub fn launch_terminal(terminal: &str, title: &str, ssh_args: &[String]) -> io::Result<()> {
	let terminal = match terminal {
		"auto" => auto_terminal(),
		other => other,
	};

	let mut command = match terminal {
		"konsole" => {
			let mut command = Command::new("konsole");
			command.arg("--new-tab").arg("-e");
			command
		}
		"kitty" => {
			let mut command = Command::new("kitty");
			command.arg("--title").arg(title);
			command
		}
		"x-terminal-emulator" => {
			let mut command = Command::new("x-terminal-emulator");
			command.arg("-e");
			command
		}
		_ => {
			let mut command = Command::new("xterm");
			command.arg("-T").arg(title).arg("-e");
			command
		}
	};

	let child = command.arg(ssh_program()).args(ssh_args).spawn()?;
	reap(child);
	Ok(())
}

fn auto_terminal() -> &'static str {
	if find_on_path("konsole").is_some() {
		"konsole"
	}
	else if find_on_path("x-terminal-emulator").is_some() {
		"x-terminal-emulator"
	}
	else {
		"xterm"
	}
}
