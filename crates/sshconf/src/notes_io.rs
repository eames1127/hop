//! Export and import of Hop's notes, so they can be carried to another machine.
//!
//! The file holds notes only, keyed by alias. Addresses, users and keys stay out of it, and an
//! import only ever updates hosts that already exist; it never adds a `Host` block.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::edit::{EditError, HostInput};
use crate::load::ConfigSet;
use crate::meta::{single_line, Forward, HostMeta};
use crate::name::is_valid_host_name;

pub const NOTES_VERSION: u32 = 1;
/// Larger files are refused before parsing; a real notes file is a few kilobytes.
pub const MAX_NOTES_FILE_BYTES: u64 = 1024 * 1024;

const MAX_HOSTS: usize = 10_000;
const MAX_DESCRIPTION_CHARS: usize = 500;
const MAX_TAGS: usize = 32;
const MAX_TAG_CHARS: usize = 64;
const MAX_FORWARDS: usize = 32;
const MAX_LABEL_CHARS: usize = 100;
const MAX_ADDED_CHARS: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NotesFile {
	pub version: u32,
	pub hosts: BTreeMap<String, NoteEntry>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct NoteEntry {
	pub description: Option<String>,
	pub tags: Vec<String>,
	pub favourite: bool,
	pub forwards: Vec<ForwardEntry>,
	pub added: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForwardEntry {
	pub local_port: u16,
	pub remote_host: String,
	pub remote_port: u16,
	#[serde(default)]
	pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotesError {
	TooLarge,
	Malformed(String),
	UnsupportedVersion(u64),
	TooManyHosts,
	InvalidAlias(String),
	InvalidField { alias: String, field: &'static str },
}

impl fmt::Display for NotesError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			NotesError::TooLarge => write!(f, "The file is too large to be a Hop notes file"),
			NotesError::Malformed(detail) => write!(f, "This is not a Hop notes file: {detail}"),
			NotesError::UnsupportedVersion(version) => {
				write!(
					f,
					"Notes file version {version} is not supported by this Hop"
				)
			}
			NotesError::TooManyHosts => write!(f, "The file lists too many hosts"),
			NotesError::InvalidAlias(alias) => {
				write!(f, "\"{}\" is not a valid host name", single_line(alias))
			}
			NotesError::InvalidField { alias, field } => {
				write!(f, "The {field} for \"{alias}\" is not valid")
			}
		}
	}
}

impl std::error::Error for NotesError {}

/// How an import treats lists that the host already has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ListMode {
	/// The file's tags and forwards replace the host's own.
	Overwrite,
	/// The file's tags and forwards are added to the host's own, without repeats.
	Merge,
}

/// Which aliases in a file would be updated, and which are skipped because they are not here.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
	pub matched: Vec<String>,
	pub unknown: Vec<String>,
}

impl From<&Forward> for ForwardEntry {
	fn from(forward: &Forward) -> ForwardEntry {
		ForwardEntry {
			local_port: forward.local_port,
			remote_host: forward.remote_host.clone(),
			remote_port: forward.remote_port,
			label: forward.label.clone(),
		}
	}
}

impl From<&HostMeta> for NoteEntry {
	fn from(meta: &HostMeta) -> NoteEntry {
		NoteEntry {
			description: meta.description.clone(),
			tags: meta.tags.clone(),
			favourite: meta.favourite,
			forwards: meta.forwards.iter().map(ForwardEntry::from).collect(),
			added: meta.added.clone(),
		}
	}
}

/// Collects the notes of every editable host. Hosts without notes are left out, so importing
/// the file elsewhere cannot clear notes that machine already has.
pub fn export_notes(set: &ConfigSet) -> NotesFile {
	let mut hosts = BTreeMap::new();
	for file in &set.files {
		for block in file.doc.blocks.iter().filter(|block| block.is_editable()) {
			let alias = &block.patterns[0];
			// The first block for an alias is the one Hop shows and edits.
			if hosts.contains_key(alias) || block.meta == HostMeta::default() {
				continue;
			}

			hosts.insert(alias.clone(), NoteEntry::from(&block.meta));
		}
	}

	NotesFile {
		version: NOTES_VERSION,
		hosts,
	}
}

pub fn to_json(file: &NotesFile) -> String {
	// Serialising plain strings, numbers and maps cannot fail.
	let mut text = serde_json::to_string_pretty(file).unwrap_or_default();
	text.push('\n');
	text
}

/// Parses and checks a notes file, returning it with every note cleaned for the config.
pub fn parse_notes(text: &str) -> Result<NotesFile, NotesError> {
	if text.len() as u64 > MAX_NOTES_FILE_BYTES {
		return Err(NotesError::TooLarge);
	}

	// Check the version first, so a newer format gets a clear message rather than a field error.
	let value: serde_json::Value =
		serde_json::from_str(text).map_err(|error| NotesError::Malformed(error.to_string()))?;
	let version = value
		.get("version")
		.and_then(serde_json::Value::as_u64)
		.ok_or_else(|| NotesError::Malformed("missing version".to_string()))?;
	if version != u64::from(NOTES_VERSION) {
		return Err(NotesError::UnsupportedVersion(version));
	}

	let file: NotesFile =
		serde_json::from_value(value).map_err(|error| NotesError::Malformed(error.to_string()))?;
	if file.hosts.len() > MAX_HOSTS {
		return Err(NotesError::TooManyHosts);
	}

	let mut hosts = BTreeMap::new();
	for (alias, entry) in file.hosts {
		if !is_valid_host_name(&alias) {
			return Err(NotesError::InvalidAlias(alias));
		}

		let entry = clean_entry(&alias, entry)?;
		hosts.insert(alias, entry);
	}

	Ok(NotesFile {
		version: NOTES_VERSION,
		hosts,
	})
}

fn clean_entry(alias: &str, entry: NoteEntry) -> Result<NoteEntry, NotesError> {
	let invalid = |field| NotesError::InvalidField {
		alias: alias.to_string(),
		field,
	};

	let description = clean_optional(entry.description, MAX_DESCRIPTION_CHARS)
		.map_err(|_| invalid("description"))?;
	let added = clean_optional(entry.added, MAX_ADDED_CHARS).map_err(|_| invalid("added date"))?;

	if entry.tags.len() > MAX_TAGS {
		return Err(invalid("tags"));
	}

	let mut tags = Vec::new();
	for tag in entry.tags {
		// Commas separate tags in the config, so one inside a tag would split it.
		let tag = single_line(&tag).replace(',', " ").trim().to_string();
		if tag.chars().count() > MAX_TAG_CHARS {
			return Err(invalid("tags"));
		}

		if !tag.is_empty() && !tags.contains(&tag) {
			tags.push(tag);
		}
	}

	if entry.forwards.len() > MAX_FORWARDS {
		return Err(invalid("port forwards"));
	}

	let mut forwards = Vec::new();
	for forward in entry.forwards {
		let label = single_line(&forward.label);
		if label.chars().count() > MAX_LABEL_CHARS {
			return Err(invalid("port forward label"));
		}

		let forward = Forward {
			local_port: forward.local_port,
			remote_host: forward.remote_host,
			remote_port: forward.remote_port,
			label,
		};
		if !forward.is_valid() {
			return Err(invalid("port forward"));
		}

		forwards.push(ForwardEntry::from(&forward));
	}

	Ok(NoteEntry {
		description,
		tags,
		favourite: entry.favourite,
		forwards,
		added,
	})
}

fn clean_optional(value: Option<String>, max_chars: usize) -> Result<Option<String>, ()> {
	let Some(value) = value
	else {
		return Ok(None);
	};

	let value = single_line(&value);
	if value.chars().count() > max_chars {
		Err(())
	}
	else if value.is_empty() {
		Ok(None)
	}
	else {
		Ok(Some(value))
	}
}

/// Splits the file's aliases into those that match an editable host here and those that do not.
pub fn preview_import(set: &ConfigSet, file: &NotesFile) -> ImportPreview {
	let mut preview = ImportPreview::default();
	for alias in file.hosts.keys() {
		if set.file_for_host(alias).is_some() {
			preview.matched.push(alias.clone());
		}
		else {
			preview.unknown.push(alias.clone());
		}
	}

	preview
}

/// The notes a host ends up with. A description or favourite in the file wins, and an existing
/// added date is kept because it records when the host was added on this machine.
pub fn merge_meta(current: &HostMeta, incoming: &NoteEntry, mode: ListMode) -> HostMeta {
	let incoming_forwards: Vec<Forward> = incoming
		.forwards
		.iter()
		.map(|forward| Forward {
			local_port: forward.local_port,
			remote_host: forward.remote_host.clone(),
			remote_port: forward.remote_port,
			label: forward.label.clone(),
		})
		.collect();

	let (tags, forwards) = match mode {
		ListMode::Overwrite => (incoming.tags.clone(), incoming_forwards),
		ListMode::Merge => (
			merge_lists(&current.tags, &incoming.tags),
			merge_lists(&current.forwards, &incoming_forwards),
		),
	};

	HostMeta {
		description: incoming
			.description
			.clone()
			.or_else(|| current.description.clone()),
		tags,
		favourite: incoming.favourite,
		forwards,
		added: current.added.clone().or_else(|| incoming.added.clone()),
	}
}

fn merge_lists<T: Clone + PartialEq>(current: &[T], incoming: &[T]) -> Vec<T> {
	let mut merged = current.to_vec();
	for item in incoming {
		if !merged.contains(item) {
			merged.push(item.clone());
		}
	}

	merged
}

/// Applies the file to every config file it touches. Returns the index into `set.files` and the
/// edited document for each file that changed, so the caller can save each one once.
pub fn apply_import(
	set: &ConfigSet,
	file: &NotesFile,
	mode: ListMode,
) -> Result<Vec<(usize, Document)>, EditError> {
	let mut changed: Vec<(usize, Document)> = Vec::new();
	for (alias, entry) in &file.hosts {
		let Some(index) = set
			.files
			.iter()
			.position(|loaded| loaded.doc.find_host(alias).is_some())
		else {
			continue;
		};

		let slot = match changed.iter().position(|(i, _)| *i == index) {
			Some(slot) => slot,
			None => {
				changed.push((index, set.files[index].doc.clone()));
				changed.len() - 1
			}
		};
		let doc = &mut changed[slot].1;
		let block = doc
			.find_host(alias)
			.ok_or_else(|| EditError::NotFound(alias.clone()))?;
		let meta = merge_meta(&block.meta, entry, mode);
		if meta == block.meta {
			continue;
		}

		let mut input = HostInput::from_block(block);
		input.meta = meta;
		doc.update_host(alias, &input)?;
	}

	changed.retain(|(index, doc)| doc.to_text() != set.files[*index].doc.to_text());
	Ok(changed)
}

#[cfg(test)]
mod tests {
	use std::path::PathBuf;

	use super::*;
	use crate::load::LoadedFile;

	fn set_of(texts: &[&str]) -> ConfigSet {
		ConfigSet {
			files: texts
				.iter()
				.enumerate()
				.map(|(index, text)| LoadedFile {
					path: PathBuf::from(format!("config{index}")),
					doc: Document::parse(text),
					included: index > 0,
				})
				.collect(),
			warnings: Vec::new(),
		}
	}

	const CONFIG: &str = "Host nas\n\t# desc: Synology\n\t# tags: home\n\t# added: 2024-01-01\n\
	                      \tHostName 10.0.0.2\n\nHost web1 web2\n\t# desc: shared\n\n\
	                      Host bare\n\tHostName bare.example\n";

	#[test]
	fn exports_notes_only_for_editable_hosts_with_notes() {
		let file = export_notes(&set_of(&[CONFIG]));
		assert_eq!(file.version, 1);
		assert_eq!(file.hosts.keys().collect::<Vec<_>>(), vec!["nas"]);
		let json = to_json(&file);
		assert!(json.contains("\"description\": \"Synology\""));
		assert!(!json.contains("10.0.0.2"));
	}

	#[test]
	fn export_round_trips_through_parse() {
		let file = export_notes(&set_of(&[CONFIG]));
		assert_eq!(parse_notes(&to_json(&file)).unwrap(), file);
	}

	#[test]
	fn refuses_wrong_version_and_unknown_fields() {
		assert_eq!(
			parse_notes(r#"{"version":2,"hosts":{}}"#),
			Err(NotesError::UnsupportedVersion(2))
		);
		assert!(matches!(
			parse_notes(r#"{"hosts":{}}"#),
			Err(NotesError::Malformed(_))
		));
		assert!(matches!(
			parse_notes(r#"{"version":1,"hosts":{"nas":{"hostName":"x"}}}"#),
			Err(NotesError::Malformed(_))
		));
		assert!(matches!(
			parse_notes("not json"),
			Err(NotesError::Malformed(_))
		));
	}

	#[test]
	fn refuses_bad_aliases_and_forwards() {
		assert!(matches!(
			parse_notes(r#"{"version":1,"hosts":{"-oProxyCommand=calc":{}}}"#),
			Err(NotesError::InvalidAlias(_))
		));
		let bad_forward = r#"{"version":1,"hosts":{"nas":{"forwards":
			[{"localPort":0,"remoteHost":"localhost","remotePort":22}]}}}"#;
		assert!(matches!(
			parse_notes(bad_forward),
			Err(NotesError::InvalidField { .. })
		));
		let bad_host = r#"{"version":1,"hosts":{"nas":{"forwards":
			[{"localPort":80,"remoteHost":"a;b","remotePort":22}]}}}"#;
		assert!(parse_notes(bad_host).is_err());
		let long = format!(
			r#"{{"version":1,"hosts":{{"nas":{{"description":"{}"}}}}}}"#,
			"x".repeat(501)
		);
		assert!(parse_notes(&long).is_err());
	}

	#[test]
	fn cleans_notes_so_they_cannot_inject_options() {
		let text = r#"{"version":1,"hosts":{"nas":{
			"description":"nice\nProxyCommand calc",
			"tags":["a,b","a b"," ","x\ny"]}}}"#;
		let file = parse_notes(text).unwrap();
		let entry = &file.hosts["nas"];
		assert_eq!(entry.description.as_deref(), Some("nice ProxyCommand calc"));
		assert_eq!(entry.tags, vec!["a b", "x y"]);
	}

	#[test]
	fn previews_matched_and_unknown_aliases() {
		let text = r#"{"version":1,"hosts":{"nas":{},"web1":{},"elsewhere":{}}}"#;
		let preview = preview_import(&set_of(&[CONFIG]), &parse_notes(text).unwrap());
		assert_eq!(preview.matched, vec!["nas"]);
		assert_eq!(preview.unknown, vec!["elsewhere", "web1"]);
	}

	#[test]
	fn merges_or_overwrites_tags() {
		let current = HostMeta {
			tags: vec!["home".to_string()],
			added: Some("2024-01-01".to_string()),
			..HostMeta::default()
		};
		let incoming = NoteEntry {
			tags: vec!["storage".to_string(), "home".to_string()],
			added: Some("2020-05-05".to_string()),
			..NoteEntry::default()
		};
		let merged = merge_meta(&current, &incoming, ListMode::Merge);
		assert_eq!(merged.tags, vec!["home", "storage"]);
		assert_eq!(merged.added.as_deref(), Some("2024-01-01"));
		let overwritten = merge_meta(&current, &incoming, ListMode::Overwrite);
		assert_eq!(overwritten.tags, vec!["storage", "home"]);
	}

	#[test]
	fn applies_to_existing_hosts_only_and_per_file() {
		let set = set_of(&[CONFIG, "Host pi\n\tHostName 10.0.0.9\n"]);
		let text = r#"{"version":1,"hosts":{
			"nas":{"tags":["storage"],"favourite":true},
			"pi":{"description":"Garden"},
			"ghost":{"description":"never added"}}}"#;
		let changed = apply_import(&set, &parse_notes(text).unwrap(), ListMode::Merge).unwrap();
		assert_eq!(changed.len(), 2);

		let main = &changed.iter().find(|(index, _)| *index == 0).unwrap().1;
		let nas = main.find_host("nas").unwrap();
		assert_eq!(nas.meta.tags, vec!["home", "storage"]);
		assert!(nas.meta.favourite);
		assert_eq!(nas.meta.description.as_deref(), Some("Synology"));
		assert!(!main.mentions_host("ghost"));
		assert!(main
			.to_text()
			.contains("Host web1 web2\n\t# desc: shared\n"));

		let included = &changed.iter().find(|(index, _)| *index == 1).unwrap().1;
		let pi = included.find_host("pi").unwrap();
		assert_eq!(pi.meta.description.as_deref(), Some("Garden"));
	}

	#[test]
	fn unchanged_notes_produce_no_save() {
		let set = set_of(&[CONFIG]);
		let file = export_notes(&set);
		let changed = apply_import(&set, &file, ListMode::Overwrite).unwrap();
		assert!(changed.is_empty());
	}
}
