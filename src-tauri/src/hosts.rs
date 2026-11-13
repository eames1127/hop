//! Turns the parsed config files into the cards the interface shows.

use std::collections::HashMap;

use chrono::NaiveDate;
use serde::Serialize;
use sshconf::{Block, BlockKind, ConfigSet, HostMeta};

use crate::history::Usage;
use crate::platform::expand_tilde;

const SECONDS_PER_DAY: i64 = 86_400;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostView {
	/// Stable for this load only: file index and first line.
	pub id: String,
	pub name: String,
	pub kind: BlockKind,
	/// A single plain alias: can be connected to and edited.
	pub editable: bool,
	pub source: String,
	pub included: bool,
	pub host_name: Option<String>,
	pub user: Option<String>,
	pub port: Option<String>,
	pub proxy_jump: Option<String>,
	pub identity_files: Vec<String>,
	pub missing_keys: Vec<String>,
	pub meta: HostMeta,
	pub last_used: Option<i64>,
	pub use_count: i64,
	pub stale: bool,
}

pub fn build_views(
	set: &ConfigSet,
	usage: &HashMap<String, Usage>,
	now: i64,
	stale_days: u32,
) -> Vec<HostView> {
	let mut views = Vec::new();
	for (file_index, file) in set.files.iter().enumerate() {
		for block in &file.doc.blocks {
			let name = match block.kind {
				BlockKind::Host => block.patterns.join(" "),
				BlockKind::Match => block.label(),
			};
			let editable = block.is_editable();
			let host_usage = usage.get(&name).filter(|_| editable);
			let last_used = host_usage.map(|usage| usage.last_used);
			let stale =
				editable && is_stale(last_used, block.meta.added.as_deref(), now, stale_days);
			let identity_files: Vec<String> = block
				.options_named("identityfile")
				.map(|path| path.trim_matches('"').to_string())
				.collect();

			views.push(HostView {
				id: format!("{file_index}:{}", block.start),
				name,
				kind: block.kind,
				editable,
				source: file.path.display().to_string(),
				included: file.included,
				host_name: owned(block, "hostname"),
				user: owned(block, "user"),
				port: owned(block, "port"),
				proxy_jump: owned(block, "proxyjump"),
				missing_keys: missing_keys(&identity_files),
				identity_files,
				meta: block.meta.clone(),
				last_used,
				use_count: host_usage.map_or(0, |usage| usage.count),
				stale,
			});
		}
	}

	views
}

fn owned(block: &Block, key: &str) -> Option<String> {
	block.option(key).map(str::to_string)
}

/// Hop only checks that key files exist; it never opens them.
pub fn missing_keys(identity_files: &[String]) -> Vec<String> {
	identity_files
		.iter()
		.filter(|path| !path.contains('%') && !path.eq_ignore_ascii_case("none"))
		.filter(|path| !expand_tilde(path).exists())
		.cloned()
		.collect()
}

pub fn is_stale(last_used: Option<i64>, added: Option<&str>, now: i64, stale_days: u32) -> bool {
	let limit = i64::from(stale_days) * SECONDS_PER_DAY;
	match last_used {
		Some(time) => now - time > limit,
		None => added
			.and_then(date_to_unix)
			.is_some_and(|added| now - added > limit),
	}
}

fn date_to_unix(date: &str) -> Option<i64> {
	let date = NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d").ok()?;
	Some(date.and_hms_opt(0, 0, 0)?.and_utc().timestamp())
}

#[cfg(test)]
mod tests {
	use std::path::PathBuf;

	use sshconf::{Document, LoadedFile};

	use super::*;

	#[test]
	fn stale_rules() {
		let day = SECONDS_PER_DAY;
		let now = 1000 * day;
		assert!(is_stale(Some(now - 91 * day), None, now, 90));
		assert!(!is_stale(Some(now - 10 * day), Some("2000-01-01"), now, 90));
		assert!(!is_stale(None, None, now, 90));
		assert!(is_stale(None, Some("1970-01-02"), now, 90));
	}

	#[test]
	fn builds_cards_with_usage() {
		let text = "Host nas\n\tIdentityFile ~/.ssh/hop-test-missing-key\n\nHost *\n\tUser me\n";
		let set = ConfigSet {
			files: vec![LoadedFile {
				path: PathBuf::from("config"),
				doc: Document::parse(text),
				included: false,
			}],
			warnings: Vec::new(),
		};
		let mut usage = HashMap::new();
		usage.insert(
			"nas".to_string(),
			Usage {
				last_used: 50,
				count: 3,
			},
		);

		let views = build_views(&set, &usage, 100, 90);
		assert_eq!(views.len(), 2);
		assert_eq!(views[0].use_count, 3);
		assert_eq!(views[0].missing_keys, vec!["~/.ssh/hop-test-missing-key"]);
		assert!(!views[1].editable);
	}
}
