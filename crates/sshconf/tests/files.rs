use std::fs;

use sshconf::{load, write_with_backup, BACKUPS_KEPT};

#[test]
fn follows_includes_relative_to_ssh_dir() {
	let home = tempfile::tempdir().unwrap();
	let ssh = home.path().join(".ssh");
	fs::create_dir_all(ssh.join("conf.d")).unwrap();
	fs::write(
		ssh.join("config"),
		"Include conf.d/*.conf\nHost main\n\tHostName m\n",
	)
	.unwrap();
	fs::write(
		ssh.join("conf.d/work.conf"),
		"Include ~/.ssh/extra\nHost work\n",
	)
	.unwrap();
	fs::write(ssh.join("extra"), "Host extra\n").unwrap();
	// A loop back to the main file must not recurse forever.
	fs::write(ssh.join("conf.d/loop.conf"), "Include ~/.ssh/config\n").unwrap();

	let set = load(&ssh.join("config"), &ssh).unwrap();
	assert_eq!(set.files.len(), 4);
	assert!(!set.files[0].included);
	assert!(set.file_for_host("work").unwrap().included);
	assert!(set.file_for_host("extra").is_some());
}

#[test]
fn missing_main_config_is_empty() {
	let home = tempfile::tempdir().unwrap();
	let ssh = home.path().join(".ssh");
	let set = load(&ssh.join("config"), &ssh).unwrap();
	assert_eq!(set.files.len(), 1);
	assert!(set.files[0].doc.blocks.is_empty());
}

#[test]
fn backup_then_atomic_write_keeps_last_ten() {
	let dir = tempfile::tempdir().unwrap();
	let config = dir.path().join("config");
	fs::write(&config, "Host a\n").unwrap();

	for index in 0..12 {
		let stamp = format!("20261106-1200{index:02}");
		let backup = write_with_backup(&config, &format!("Host b{index}\n"), &stamp).unwrap();
		assert!(backup.unwrap().exists());
	}

	assert_eq!(fs::read_to_string(&config).unwrap(), "Host b11\n");
	let mut backups: Vec<String> = fs::read_dir(dir.path())
		.unwrap()
		.map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
		.filter(|name| name.starts_with("config.hop-backup-"))
		.collect();
	backups.sort();
	assert_eq!(backups.len(), BACKUPS_KEPT);
	assert_eq!(backups[0], "config.hop-backup-20261106-120002");
	let oldest_kept = fs::read_to_string(dir.path().join(&backups[0])).unwrap();
	assert_eq!(oldest_kept, "Host b1\n");
	assert!(!dir.path().join(".config.hop-tmp").exists());
}

#[test]
fn same_second_backups_do_not_overwrite() {
	let dir = tempfile::tempdir().unwrap();
	let config = dir.path().join("config");
	fs::write(&config, "one\n").unwrap();
	let first = write_with_backup(&config, "two\n", "20261106-120000")
		.unwrap()
		.unwrap();
	let second = write_with_backup(&config, "three\n", "20261106-120000")
		.unwrap()
		.unwrap();
	assert_ne!(first, second);
	assert_eq!(fs::read_to_string(second).unwrap(), "two\n");
}

#[test]
fn leftover_temp_file_is_replaced() {
	let dir = tempfile::tempdir().unwrap();
	let config = dir.path().join("config");
	fs::write(
		dir.path().join(".config.hop-tmp"),
		"stale half-written text",
	)
	.unwrap();
	write_with_backup(&config, "Host a\n", "20261106-120000").unwrap();
	assert_eq!(fs::read_to_string(&config).unwrap(), "Host a\n");
	assert!(!dir.path().join(".config.hop-tmp").exists());
}

#[cfg(unix)]
#[test]
fn new_config_is_private() {
	use std::os::unix::fs::PermissionsExt;

	let dir = tempfile::tempdir().unwrap();
	let config = dir.path().join("config");
	write_with_backup(&config, "Host a\n", "20261106-120000").unwrap();
	let mode = fs::metadata(&config).unwrap().permissions().mode();
	assert_eq!(mode & 0o777, 0o600);
}
