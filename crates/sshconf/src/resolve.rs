use std::fmt;
use std::path::Path;
use std::process::Command;

use serde::Serialize;

use crate::name::is_valid_host_name;

/// A host's final settings as OpenSSH itself sees them, from `ssh -G`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedHost {
	pub host_name: String,
	pub user: Option<String>,
	pub port: u16,
	pub proxy_jump: Option<String>,
	pub identity_files: Vec<String>,
}

#[derive(Debug)]
pub enum ResolveError {
	InvalidName(String),
	Io(std::io::Error),
	Failed(String),
}

impl fmt::Display for ResolveError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			ResolveError::InvalidName(name) => write!(f, "\"{name}\" is not a valid host name"),
			ResolveError::Io(error) => write!(f, "Could not run ssh: {error}"),
			ResolveError::Failed(message) => write!(f, "ssh -G failed: {message}"),
		}
	}
}

impl std::error::Error for ResolveError {}

/// Runs `ssh -G <name>` with the given ssh program. The name is checked first.
pub fn resolve(ssh: &Path, name: &str) -> Result<ResolvedHost, ResolveError> {
	if !is_valid_host_name(name) {
		return Err(ResolveError::InvalidName(name.to_string()));
	}

	let mut command = Command::new(ssh);
	command.arg("-G").arg(name);
	hide_console(&mut command);
	let output = command.output().map_err(ResolveError::Io)?;
	if !output.status.success() {
		let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
		return Err(ResolveError::Failed(message));
	}

	Ok(parse_ssh_g(&String::from_utf8_lossy(&output.stdout)))
}

#[cfg(windows)]
fn hide_console(command: &mut Command) {
	use std::os::windows::process::CommandExt;

	const CREATE_NO_WINDOW: u32 = 0x0800_0000;
	command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console(_command: &mut Command) {}

pub fn parse_ssh_g(output: &str) -> ResolvedHost {
	let mut host = ResolvedHost {
		port: 22,
		..ResolvedHost::default()
	};
	for line in output.lines() {
		let Some((key, value)) = line.trim().split_once(' ')
		else {
			continue;
		};
		let value = value.trim();
		match key {
			"hostname" => host.host_name = value.to_string(),
			"user" => host.user = Some(value.to_string()),
			"port" => host.port = value.parse().unwrap_or(22),
			"proxyjump" if value != "none" => host.proxy_jump = Some(value.to_string()),
			"identityfile" => host.identity_files.push(value.to_string()),
			_ => {}
		}
	}

	host
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn reads_ssh_g_output() {
		let output = "host nas\nhostname 10.0.0.2\nuser alex\nport 2222\n\
			identityfile ~/.ssh/id_ed25519\nproxyjump bastion\n";
		let host = parse_ssh_g(output);
		assert_eq!(host.host_name, "10.0.0.2");
		assert_eq!(host.user.as_deref(), Some("alex"));
		assert_eq!(host.port, 2222);
		assert_eq!(host.proxy_jump.as_deref(), Some("bastion"));
		assert_eq!(host.identity_files, vec!["~/.ssh/id_ed25519"]);
	}

	#[test]
	fn refuses_bad_names_before_running_anything() {
		let result = resolve(Path::new("ssh"), "nas; rm -rf ~");
		assert!(matches!(result, Err(ResolveError::InvalidName(_))));
	}
}
