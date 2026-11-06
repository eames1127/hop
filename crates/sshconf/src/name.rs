/// Checks a host alias before it is used in a command or written to the config.
///
/// Only `[A-Za-z0-9._-]` is allowed, and a leading `-` is refused so a name can never be read
/// as an option by `ssh` or a terminal.
pub fn is_valid_host_name(name: &str) -> bool {
	if name.is_empty() || name.len() > 253 {
		return false;
	}

	if name.starts_with('-') {
		return false;
	}

	name.chars()
		.all(|c| c.is_ascii_alphanumeric() || ".-_".contains(c))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn accepts_plain_names() {
		assert!(is_valid_host_name("nas"));
		assert!(is_valid_host_name("web-01.example.com"));
		assert!(is_valid_host_name("pi_4"));
	}

	#[test]
	fn refuses_injection_attempts() {
		assert!(!is_valid_host_name(""));
		assert!(!is_valid_host_name("nas; rm -rf ~"));
		assert!(!is_valid_host_name("nas && whoami"));
		assert!(!is_valid_host_name("$(reboot)"));
		assert!(!is_valid_host_name("-oProxyCommand=calc"));
		assert!(!is_valid_host_name("-v"));
		assert!(!is_valid_host_name("host name"));
		assert!(!is_valid_host_name("nas\nProxyCommand x"));
		assert!(!is_valid_host_name("*"));
	}
}
