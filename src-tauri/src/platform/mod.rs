//! Everything that differs between Windows and Linux: paths and how a terminal is opened.

use std::path::PathBuf;
use std::process::Child;
use std::thread;

use serde::Serialize;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{launch_terminal, ssh_keygen_program, ssh_program, TERMINALS};

#[cfg(not(windows))]
mod linux;
#[cfg(not(windows))]
pub use linux::{launch_terminal, ssh_keygen_program, ssh_program, TERMINALS};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalChoice {
	pub id: &'static str,
	pub label: &'static str,
}

pub fn terminal_choices() -> Vec<TerminalChoice> {
	TERMINALS
		.iter()
		.map(|(id, label)| TerminalChoice { id, label })
		.collect()
}

pub fn is_known_terminal(id: &str) -> bool {
	TERMINALS.iter().any(|(known, _)| *known == id)
}

pub fn home_dir() -> Option<PathBuf> {
	let key = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
	std::env::var_os(key)
		.filter(|value| !value.is_empty())
		.map(PathBuf::from)
}

/// `HOP_SSH_DIR` points Hop at a different folder, so it can be tried against a copy of a
/// config without touching the real one.
pub fn ssh_dir() -> Option<PathBuf> {
	if let Some(dir) = std::env::var_os("HOP_SSH_DIR").filter(|value| !value.is_empty()) {
		return Some(PathBuf::from(dir));
	}

	home_dir().map(|home| home.join(".ssh"))
}

/// Expands a leading `~/` the way OpenSSH does, so the same key path works on both machines.
pub fn expand_tilde(path: &str) -> PathBuf {
	if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
		if let Some(home) = home_dir() {
			return home.join(rest);
		}
	}

	PathBuf::from(path)
}

/// The terminal returns straight away; waiting on it in the background stops it lingering as a
/// zombie process on Linux.
fn reap(mut child: Child) {
	thread::spawn(move || {
		if let Err(error) = child.wait() {
			eprintln!("hop: could not wait for terminal process: {error}");
		}
	});
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn auto_is_always_offered() {
		assert!(is_known_terminal("auto"));
		assert!(!is_known_terminal("bash -c evil"));
	}
}
