use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// How many `*.hop-backup-*` copies are kept next to each config file.
pub const BACKUPS_KEPT: usize = 10;

/// Backs up the current file, then writes the new text to a temporary file and swaps it in, so
/// a crash mid-save cannot leave a half-written config. Returns the backup path, if one was made.
///
/// `stamp` names the backup (for example `20261106-181500`); the caller supplies it so this crate
/// needs no clock.
pub fn write_with_backup(path: &Path, text: &str, stamp: &str) -> io::Result<Option<PathBuf>> {
	let dir = path
		.parent()
		.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "config path has no folder"))?;
	let file_name = path
		.file_name()
		.and_then(|name| name.to_str())
		.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "config path has no name"))?;
	fs::create_dir_all(dir)?;

	let backup = if path.exists() {
		let backup = free_backup_path(dir, file_name, stamp);
		fs::copy(path, &backup)?;
		prune_backups(dir, file_name)?;
		Some(backup)
	}
	else {
		None
	};

	let temp = dir.join(format!(".{file_name}.hop-tmp"));
	{
		let mut file = fs::File::create(&temp)?;
		file.write_all(text.as_bytes())?;
		file.sync_all()?;
	}
	copy_permissions(path, &temp)?;
	fs::rename(&temp, path)?;
	Ok(backup)
}

fn backup_prefix(file_name: &str) -> String {
	format!("{file_name}.hop-backup-")
}

fn free_backup_path(dir: &Path, file_name: &str, stamp: &str) -> PathBuf {
	let first = dir.join(format!("{}{stamp}", backup_prefix(file_name)));
	if !first.exists() {
		return first;
	}

	(2..)
		.map(|count| dir.join(format!("{}{stamp}-{count}", backup_prefix(file_name))))
		.find(|path| !path.exists())
		.unwrap_or(first)
}

/// Deletes the oldest backups beyond [`BACKUPS_KEPT`]. Stamps sort by time, so names sort too.
fn prune_backups(dir: &Path, file_name: &str) -> io::Result<()> {
	let prefix = backup_prefix(file_name);
	let mut backups: Vec<PathBuf> = fs::read_dir(dir)?
		.filter_map(Result::ok)
		.map(|entry| entry.path())
		.filter(|path| {
			path.file_name()
				.and_then(|name| name.to_str())
				.is_some_and(|name| name.starts_with(&prefix))
		})
		.collect();
	backups.sort();
	let excess = backups.len().saturating_sub(BACKUPS_KEPT);
	for old in backups.into_iter().take(excess) {
		fs::remove_file(old)?;
	}

	Ok(())
}

/// OpenSSH refuses configs others can write, so a new file keeps the old file's mode.
#[cfg(unix)]
fn copy_permissions(original: &Path, temp: &Path) -> io::Result<()> {
	use std::os::unix::fs::PermissionsExt;

	let mode = match fs::metadata(original) {
		Ok(metadata) => metadata.permissions().mode(),
		Err(_) => 0o600,
	};
	fs::set_permissions(temp, fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn copy_permissions(_original: &Path, _temp: &Path) -> io::Result<()> {
	Ok(())
}
