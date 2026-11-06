use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::document::Document;

/// Nested `Include` deeper than this is ignored, matching OpenSSH's own limit.
const MAX_INCLUDE_DEPTH: usize = 16;

#[derive(Debug, Clone)]
pub struct LoadedFile {
	pub path: PathBuf,
	pub doc: Document,
	/// False for the main config, true for files reached through `Include`.
	pub included: bool,
}

/// The main config and every file it includes, main first.
#[derive(Debug, Clone, Default)]
pub struct ConfigSet {
	pub files: Vec<LoadedFile>,
	pub warnings: Vec<String>,
}

impl ConfigSet {
	pub fn file_for_host(&self, name: &str) -> Option<&LoadedFile> {
		self.files
			.iter()
			.find(|file| file.doc.find_host(name).is_some())
	}

	pub fn mentions_host(&self, name: &str) -> bool {
		self.files.iter().any(|file| file.doc.mentions_host(name))
	}
}

/// Reads the main config and follows `Include` lines. A missing main file is treated as empty.
pub fn load(main: &Path, ssh_dir: &Path) -> io::Result<ConfigSet> {
	let mut set = ConfigSet::default();
	let text = match fs::read_to_string(main) {
		Ok(text) => text,
		Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
		Err(error) => return Err(error),
	};

	let mut visited = HashSet::new();
	visited.insert(canonical(main));
	let doc = Document::parse(&text);
	let includes = doc.includes();
	set.files.push(LoadedFile {
		path: main.to_path_buf(),
		doc,
		included: false,
	});
	follow_includes(&includes, ssh_dir, 1, &mut visited, &mut set);
	Ok(set)
}

fn follow_includes(
	includes: &[String],
	ssh_dir: &Path,
	depth: usize,
	visited: &mut HashSet<PathBuf>,
	set: &mut ConfigSet,
) {
	if depth > MAX_INCLUDE_DEPTH {
		set.warnings
			.push("Include files are nested too deeply; some were skipped".to_string());
		return;
	}

	for pattern in includes {
		for path in expand_include(pattern, ssh_dir, &mut set.warnings) {
			if !visited.insert(canonical(&path)) {
				continue;
			}

			match fs::read_to_string(&path) {
				Ok(text) => {
					let doc = Document::parse(&text);
					let nested = doc.includes();
					set.files.push(LoadedFile {
						path,
						doc,
						included: true,
					});
					follow_includes(&nested, ssh_dir, depth + 1, visited, set);
				}
				Err(error) => {
					set.warnings
						.push(format!("Could not read {}: {error}", path.display()));
				}
			}
		}
	}
}

/// Resolves `~` and paths relative to the `.ssh` folder, then expands glob patterns.
fn expand_include(pattern: &str, ssh_dir: &Path, warnings: &mut Vec<String>) -> Vec<PathBuf> {
	let home = ssh_dir.parent().unwrap_or(ssh_dir);
	let path = if let Some(rest) = pattern.strip_prefix("~/") {
		home.join(rest)
	}
	else {
		let path = PathBuf::from(pattern);
		if path.is_absolute() {
			path
		}
		else {
			ssh_dir.join(path)
		}
	};

	let Some(text) = path.to_str()
	else {
		warnings.push(format!(
			"Skipped an Include path that is not valid text: {pattern}"
		));
		return Vec::new();
	};

	match glob::glob(text) {
		Ok(paths) => paths
			.filter_map(Result::ok)
			.filter(|path| path.is_file())
			.collect(),
		Err(error) => {
			warnings.push(format!("Skipped Include \"{pattern}\": {error}"));
			Vec::new()
		}
	}
}

fn canonical(path: &Path) -> PathBuf {
	fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
