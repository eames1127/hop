//! Moves hosts from one computer's SSH folder to another's through a notes file, the way the app
//! does, and asks OpenSSH what the result means.

use std::fs;
use std::path::Path;
use std::process::Command;

use sshconf::{
	apply_import, export_notes, load, parse_notes, preview_import, to_json, write_with_backup,
	ListMode,
};

const OLD_COMPUTER: &str = "\
Include conf.d/*.conf

Host nas
	# desc: Synology NAS
	# tags: home, storage
	# fav: true
	# forward: 5001:localhost:5001 Admin UI
	# added: 2024-01-01
	HostName 192.168.1.20
	User dan
	Port 2222
	IdentityFile \"~/.ssh/home key\"

Host web1 web2
	HostName shared.example

Host *
	ServerAliveInterval 30
";

const OLD_INCLUDED: &str = "\
Host work
	# desc: Work box
	HostName work.example
	ProxyJump nas
	IdentityFile ~/.ssh/id_work
";

fn write_home(home: &Path, config: &str, included: Option<&str>) {
	let ssh = home.join(".ssh");
	fs::create_dir_all(ssh.join("conf.d")).unwrap();
	fs::write(ssh.join("config"), config).unwrap();
	if let Some(text) = included {
		fs::write(ssh.join("conf.d/work.conf"), text).unwrap();
	}
}

/// What `ssh -G` resolves for a host, read from that computer's config and home folder.
fn ssh_g(home: &Path, host: &str) -> Option<String> {
	let output = Command::new("ssh")
		.arg("-G")
		.arg("-F")
		.arg(home.join(".ssh/config"))
		.arg(host)
		.env("HOME", home)
		.output()
		.ok()?;
	assert!(
		output.status.success(),
		"ssh -G {host} failed: {}",
		String::from_utf8_lossy(&output.stderr)
	);
	Some(String::from_utf8(output.stdout).unwrap())
}

fn has_line(output: &str, line: &str) -> bool {
	output.lines().any(|candidate| candidate == line)
}

#[test]
fn hosts_move_to_a_computer_that_has_none_of_them() {
	let old = tempfile::tempdir().unwrap();
	write_home(old.path(), OLD_COMPUTER, Some(OLD_INCLUDED));
	let old_ssh = old.path().join(".ssh");
	let exported = to_json(&export_notes(
		&load(&old_ssh.join("config"), &old_ssh).unwrap(),
		true,
	));
	assert!(!exported.contains("ServerAliveInterval"));

	// The new computer has its own wildcard defaults and one unrelated host.
	let new = tempfile::tempdir().unwrap();
	let new_config = "Host pi\n\tHostName 10.0.0.9\n\nHost *\n\tUser fallback\n";
	write_home(new.path(), new_config, None);
	let new_ssh = new.path().join(".ssh");
	let set = load(&new_ssh.join("config"), &new_ssh).unwrap();
	let notes = parse_notes(&exported).unwrap();

	let preview = preview_import(&set, &notes);
	assert!(preview.matched.is_empty());
	let new_names: Vec<_> = preview.new.iter().map(|host| host.name.as_str()).collect();
	assert_eq!(new_names, vec!["nas", "work"]);

	let changed = apply_import(&set, &notes, ListMode::Merge, true, "2026-11-13").unwrap();
	assert_eq!(changed.len(), 1);
	for (index, doc) in &changed {
		write_with_backup(&set.files[*index].path, &doc.to_text(), "test").unwrap();
	}

	let saved = fs::read_to_string(new_ssh.join("config")).unwrap();
	assert!(saved.starts_with(new_config.split("Host *").next().unwrap()));
	assert!(saved.trim_end().ends_with("Host *\n\tUser fallback"));

	let reloaded = load(&new_ssh.join("config"), &new_ssh).unwrap();
	let nas = reloaded
		.file_for_host("nas")
		.unwrap()
		.doc
		.find_host("nas")
		.unwrap();
	assert_eq!(nas.meta.description.as_deref(), Some("Synology NAS"));
	assert_eq!(nas.meta.tags, vec!["home", "storage"]);
	assert!(nas.meta.favourite);
	assert_eq!(nas.meta.forwards.len(), 1);
	assert_eq!(nas.meta.added.as_deref(), Some("2024-01-01"));

	// Importing the same file again changes nothing.
	let again = apply_import(&reloaded, &notes, ListMode::Merge, true, "2026-11-14").unwrap();
	assert!(again.is_empty());

	let Some(nas) = ssh_g(new.path(), "nas")
	else {
		eprintln!("ssh is not installed; skipping the OpenSSH checks");
		return;
	};
	assert!(has_line(&nas, "hostname 192.168.1.20"), "{nas}");
	assert!(has_line(&nas, "user dan"), "{nas}");
	assert!(has_line(&nas, "port 2222"), "{nas}");
	// `ssh -G` prints key paths unexpanded; the space survives the quoting.
	assert!(has_line(&nas, "identityfile ~/.ssh/home key"), "{nas}");

	let work = ssh_g(new.path(), "work").unwrap();
	assert!(has_line(&work, "hostname work.example"), "{work}");
	assert!(has_line(&work, "proxyjump nas"), "{work}");
	// The new computer's own `Host *` still applies to imported hosts.
	assert!(has_line(&work, "user fallback"), "{work}");
}

#[test]
fn a_notes_only_file_adds_hosts_without_addresses() {
	let old = tempfile::tempdir().unwrap();
	write_home(old.path(), OLD_COMPUTER, Some(OLD_INCLUDED));
	let old_ssh = old.path().join(".ssh");
	let exported = to_json(&export_notes(
		&load(&old_ssh.join("config"), &old_ssh).unwrap(),
		false,
	));
	assert!(exported.contains("\"version\": 1"));
	assert!(!exported.contains("192.168.1.20"));
	assert!(!exported.contains("id_work"));

	let new = tempfile::tempdir().unwrap();
	write_home(new.path(), "", None);
	let new_ssh = new.path().join(".ssh");
	let set = load(&new_ssh.join("config"), &new_ssh).unwrap();
	let changed = apply_import(
		&set,
		&parse_notes(&exported).unwrap(),
		ListMode::Merge,
		true,
		"2026-11-13",
	)
	.unwrap();
	let text = changed[0].1.to_text();
	assert!(text.contains("Host nas\n"));
	assert!(text.contains("# desc: Work box"));
	assert!(!text.contains("HostName"));

	fs::write(new_ssh.join("config"), &text).unwrap();
	if let Some(work) = ssh_g(new.path(), "work") {
		// With no address, OpenSSH falls back to the alias, as it did for the hand-made blanks.
		assert!(has_line(&work, "hostname work"), "{work}");
	}
}
