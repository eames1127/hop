//! Export and import of Hop's notes, so they can be carried to another machine.
//!
//! The file holds notes keyed by alias and, from version 2, each host's connection details
//! (address, user, port, key file path and jump host). Key file paths are only paths: a key never
//! leaves the computer it was made on. An import updates the notes of hosts that already exist
//! and can add a `Host` block for each one that does not.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::edit::{EditError, HostInput};
use crate::load::ConfigSet;
use crate::meta::{single_line, Forward, HostMeta};
use crate::name::is_valid_host_name;

/// The version written when the file holds connection details.
pub const NOTES_VERSION: u32 = 2;
/// Notes only. Still written when connection details are left out, so older Hops can read it.
const NOTES_ONLY_VERSION: u32 = 1;
/// Larger files are refused before parsing; a real notes file is a few kilobytes.
pub const MAX_NOTES_FILE_BYTES: u64 = 1024 * 1024;

const MAX_HOSTS: usize = 10_000;
const MAX_DESCRIPTION_CHARS: usize = 500;
const MAX_TAGS: usize = 32;
const MAX_TAG_CHARS: usize = 64;
const MAX_FORWARDS: usize = 32;
const MAX_LABEL_CHARS: usize = 100;
const MAX_ADDED_CHARS: usize = 32;
const MAX_CONNECTION_VALUE_CHARS: usize = 255;
const MAX_IDENTITY_FILE_CHARS: usize = 1024;

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
	/// Left out of notes-only files, so they stay readable by version 1 Hops.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub connection: Option<ConnectionEntry>,
}

/// How to reach a host, used only when an import adds it. Existing hosts keep their own.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct ConnectionEntry {
	#[serde(skip_serializing_if = "Option::is_none")]
	pub host_name: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub user: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub port: Option<u16>,
	/// The key's path as the config writes it, such as `~/.ssh/id_ed25519`. Never the key.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub identity_file: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub proxy_jump: Option<String>,
}

impl ConnectionEntry {
	fn from_input(input: &HostInput) -> Option<ConnectionEntry> {
		let text = |value: Option<&str>| {
			value
				.map(str::trim)
				.filter(|value| !value.is_empty())
				.map(str::to_string)
		};
		let entry = ConnectionEntry {
			host_name: text(Some(&input.host_name)),
			user: text(input.user.as_deref()),
			port: input.port,
			identity_file: text(input.identity_file.as_deref()),
			proxy_jump: text(input.proxy_jump.as_deref()),
		};
		(entry != ConnectionEntry::default()).then_some(entry)
	}

	fn to_input(&self, name: &str, meta: HostMeta) -> HostInput {
		HostInput {
			name: name.to_string(),
			host_name: self.host_name.clone().unwrap_or_default(),
			user: self.user.clone(),
			port: self.port,
			identity_file: self.identity_file.clone(),
			proxy_jump: self.proxy_jump.clone(),
			meta,
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForwardEntry {
	pub local_port: u16,
	pub remote_host: String,
	pub remote_port: u16,
	#[serde(default)]
	pub label: String,
	/// Left out when false, so files without auto-start still import into older versions.
	#[serde(default, skip_serializing_if = "is_false")]
	pub auto_start: bool,
}

fn is_false(value: &bool) -> bool {
	!*value
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

/// What an import would do with each alias in the file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
	/// Editable hosts here whose notes would be updated.
	pub matched: Vec<String>,
	/// Hosts not in the config at all, which the import can add.
	pub new: Vec<NewHost>,
	/// Named by a block Hop does not edit, such as `Host web1 web2`, so they are skipped.
	pub conflicting: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewHost {
	pub name: String,
	pub host_name: Option<String>,
	pub identity_file: Option<String>,
	/// Set by the app, which can look at this computer's files: the key file is not here yet.
	pub key_missing: bool,
}

impl From<&Forward> for ForwardEntry {
	fn from(forward: &Forward) -> ForwardEntry {
		ForwardEntry {
			local_port: forward.local_port,
			remote_host: forward.remote_host.clone(),
			remote_port: forward.remote_port,
			label: forward.label.clone(),
			auto_start: forward.auto_start,
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
			connection: None,
		}
	}
}

/// Collects the notes of every editable host, and their connection details when asked. Hosts
/// with nothing to export are left out, so importing the file elsewhere cannot clear notes that
/// machine already has.
pub fn export_notes(set: &ConfigSet, include_connection: bool) -> NotesFile {
	let mut hosts = BTreeMap::new();
	for file in &set.files {
		for block in file.doc.blocks.iter().filter(|block| block.is_editable()) {
			let alias = &block.patterns[0];
			// The first block for an alias is the one Hop shows and edits.
			if hosts.contains_key(alias) {
				continue;
			}

			let mut entry = NoteEntry::from(&block.meta);
			if include_connection {
				entry.connection = ConnectionEntry::from_input(&HostInput::from_block(block));
			}

			if entry != NoteEntry::default() {
				hosts.insert(alias.clone(), entry);
			}
		}
	}

	let has_connection = hosts.values().any(|entry| entry.connection.is_some());
	NotesFile {
		version: if has_connection {
			NOTES_VERSION
		}
		else {
			NOTES_ONLY_VERSION
		},
		hosts,
	}
}

/// Which hosts an export covers, so a file shared with someone else holds only what it should.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum ExportSelection {
	All,
	Hosts {
		names: Vec<String>,
	},
	/// Hosts with at least one of these tags.
	Tags {
		tags: Vec<String>,
	},
}

impl ExportSelection {
	fn includes(&self, alias: &str, entry: &NoteEntry) -> bool {
		match self {
			ExportSelection::All => true,
			ExportSelection::Hosts { names } => names.iter().any(|name| name == alias),
			ExportSelection::Tags { tags } => entry.tags.iter().any(|tag| tags.contains(tag)),
		}
	}
}

/// Like `export_notes`, but limited to the chosen hosts.
pub fn export_selected(
	set: &ConfigSet,
	selection: &ExportSelection,
	include_connection: bool,
) -> NotesFile {
	let mut file = export_notes(set, include_connection);
	file.hosts
		.retain(|alias, entry| selection.includes(alias, entry));
	if file.hosts.values().all(|entry| entry.connection.is_none()) {
		file.version = NOTES_ONLY_VERSION;
	}

	file
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
	if version != u64::from(NOTES_VERSION) && version != u64::from(NOTES_ONLY_VERSION) {
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
		version: file.version,
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
			auto_start: forward.auto_start,
		};
		if !forward.is_valid() {
			return Err(invalid("port forward"));
		}

		forwards.push(ForwardEntry::from(&forward));
	}

	let connection = match entry.connection {
		Some(connection) => clean_connection(alias, connection)?,
		None => None,
	};

	Ok(NoteEntry {
		description,
		tags,
		favourite: entry.favourite,
		forwards,
		added,
		connection,
	})
}

/// Holds imported connection details to the host form's rules, and refuses values starting
/// with `-` so nothing in a shared file can be read by `ssh` as an option.
fn clean_connection(
	alias: &str,
	connection: ConnectionEntry,
) -> Result<Option<ConnectionEntry>, NotesError> {
	let invalid = |field| NotesError::InvalidField {
		alias: alias.to_string(),
		field,
	};
	let value = |value: Option<String>, max_chars: usize, field| match value {
		None => Ok(None),
		Some(value) => {
			let value = value.trim().to_string();
			if value.chars().count() > max_chars || value.starts_with('-') {
				Err(invalid(field))
			}
			else {
				Ok((!value.is_empty()).then_some(value))
			}
		}
	};

	let connection = ConnectionEntry {
		host_name: value(connection.host_name, MAX_CONNECTION_VALUE_CHARS, "address")?,
		user: value(connection.user, MAX_CONNECTION_VALUE_CHARS, "user")?,
		port: connection.port,
		identity_file: value(
			connection.identity_file,
			MAX_IDENTITY_FILE_CHARS,
			"key file",
		)?,
		proxy_jump: value(
			connection.proxy_jump,
			MAX_CONNECTION_VALUE_CHARS,
			"jump host",
		)?,
	};
	connection
		.to_input(alias, HostMeta::default())
		.validate()
		.map_err(|error| match error {
			EditError::InvalidField(field) => invalid(field),
			_ => NotesError::InvalidAlias(alias.to_string()),
		})?;

	Ok((connection != ConnectionEntry::default()).then_some(connection))
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

/// Sorts the file's aliases into hosts to update, hosts to add and names already used by a
/// block Hop does not edit.
pub fn preview_import(set: &ConfigSet, file: &NotesFile) -> ImportPreview {
	let mut preview = ImportPreview::default();
	for (alias, entry) in &file.hosts {
		if set.file_for_host(alias).is_some() {
			preview.matched.push(alias.clone());
		}
		else if set.mentions_host(alias) {
			preview.conflicting.push(alias.clone());
		}
		else {
			let connection = entry.connection.as_ref();
			preview.new.push(NewHost {
				name: alias.clone(),
				host_name: connection.and_then(|connection| connection.host_name.clone()),
				identity_file: connection.and_then(|connection| connection.identity_file.clone()),
				key_missing: false,
			});
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
			auto_start: forward.auto_start,
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

/// Applies the file to every config file it touches. With `add_new`, hosts that are not in the
/// config yet are added to the main file, dated `today` unless the file has an added date.
/// Returns the index into `set.files` and the edited document for each file that changed, so
/// the caller can save each one once.
pub fn apply_import(
	set: &ConfigSet,
	file: &NotesFile,
	mode: ListMode,
	add_new: bool,
	today: &str,
) -> Result<Vec<(usize, Document)>, EditError> {
	let mut changed: Vec<(usize, Document)> = Vec::new();
	let mut doc_for = |index: usize| -> usize {
		match changed.iter().position(|(i, _)| *i == index) {
			Some(slot) => slot,
			None => {
				changed.push((index, set.files[index].doc.clone()));
				changed.len() - 1
			}
		}
	};
	let mut edits: Vec<(usize, &String, &NoteEntry)> = Vec::new();
	let mut additions: Vec<(&String, &NoteEntry)> = Vec::new();
	for (alias, entry) in &file.hosts {
		if let Some(index) = set
			.files
			.iter()
			.position(|loaded| loaded.doc.find_host(alias).is_some())
		{
			edits.push((doc_for(index), alias, entry));
		}
		else if add_new && !set.mentions_host(alias) && !set.files.is_empty() {
			additions.push((alias, entry));
		}
	}

	// New hosts always go in the main config, never an included file.
	let main_slot = (!additions.is_empty()).then(|| doc_for(0));
	for (slot, alias, entry) in edits {
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

	if let Some(slot) = main_slot {
		let doc = &mut changed[slot].1;
		for (alias, entry) in additions {
			let mut meta = merge_meta(&HostMeta::default(), entry, ListMode::Overwrite);
			meta.added = meta.added.or_else(|| Some(today.to_string()));
			let input = match &entry.connection {
				Some(connection) => connection.to_input(alias, meta),
				None => ConnectionEntry::default().to_input(alias, meta),
			};
			doc.add_host(&input)?;
		}
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
		let file = export_notes(&set_of(&[CONFIG]), false);
		assert_eq!(file.version, 1);
		assert_eq!(file.hosts.keys().collect::<Vec<_>>(), vec!["nas"]);
		let json = to_json(&file);
		assert!(json.contains("\"description\": \"Synology\""));
		assert!(!json.contains("10.0.0.2"));
	}

	#[test]
	fn exports_only_the_selected_hosts_or_tags() {
		let config = "Host nas\n\t# tags: home, storage\n\nHost pi\n\t# tags: home\n\n\
		              Host web\n\t# tags: work\n\nHost bare\n";
		let set = set_of(&[config]);
		let aliases = |selection: ExportSelection| {
			export_selected(&set, &selection, false)
				.hosts
				.into_keys()
				.collect::<Vec<_>>()
		};

		assert_eq!(aliases(ExportSelection::All), vec!["nas", "pi", "web"]);
		let names = vec!["web".to_string(), "bare".to_string()];
		assert_eq!(aliases(ExportSelection::Hosts { names }), vec!["web"]);
		let tags = vec!["storage".to_string(), "work".to_string()];
		assert_eq!(aliases(ExportSelection::Tags { tags }), vec!["nas", "web"]);
		let none = ExportSelection::Tags { tags: Vec::new() };
		assert!(aliases(none).is_empty());
	}

	#[test]
	fn reads_export_selections_from_the_page() {
		let json = r#"{"kind":"tags","tags":["home"]}"#;
		let selection: ExportSelection = serde_json::from_str(json).unwrap();
		assert_eq!(
			selection,
			ExportSelection::Tags {
				tags: vec!["home".to_string()]
			}
		);
		let extra = r#"{"kind":"hosts","names":[],"path":"/etc"}"#;
		assert!(serde_json::from_str::<ExportSelection>(extra).is_err());
	}

	#[test]
	fn export_round_trips_through_parse() {
		let file = export_notes(&set_of(&[CONFIG]), false);
		assert_eq!(parse_notes(&to_json(&file)).unwrap(), file);
		let file = export_notes(&set_of(&[CONFIG]), true);
		assert_eq!(parse_notes(&to_json(&file)).unwrap(), file);
	}

	#[test]
	fn exports_connection_details_only_when_asked() {
		let set = set_of(&[CONFIG]);
		let notes_only = export_notes(&set, false);
		assert_eq!(notes_only.version, 1);
		assert!(!to_json(&notes_only).contains("connection"));

		let file = export_notes(&set, true);
		assert_eq!(file.version, 2);
		// A host with an address but no notes is now worth exporting.
		assert_eq!(file.hosts.keys().collect::<Vec<_>>(), vec!["bare", "nas"]);
		let nas = file.hosts["nas"].connection.as_ref().unwrap();
		assert_eq!(nas.host_name.as_deref(), Some("10.0.0.2"));
		assert_eq!(nas.user, None);

		// Choosing only hosts without connection details still writes a version 1 file.
		let config = "Host nas\n\t# tags: home\n\nHost web\n\tHostName web.example\n";
		let tags = vec!["home".to_string()];
		let file = export_selected(&set_of(&[config]), &ExportSelection::Tags { tags }, true);
		assert_eq!(file.version, 1);
	}

	#[test]
	fn exports_key_paths_never_keys() {
		let config = "Host nas\n\tHostName 10.0.0.2\n\tUser alex\n\tPort 2222\n\
		              \tIdentityFile \"~/.ssh/my key\"\n\tProxyJump bastion\n";
		let json = to_json(&export_notes(&set_of(&[config]), true));
		assert!(json.contains("\"identityFile\": \"~/.ssh/my key\""));
		assert!(json.contains("\"port\": 2222"));
		assert!(json.contains("\"proxyJump\": \"bastion\""));
	}

	#[test]
	fn refuses_wrong_version_and_unknown_fields() {
		assert_eq!(
			parse_notes(r#"{"version":3,"hosts":{}}"#),
			Err(NotesError::UnsupportedVersion(3))
		);
		assert!(parse_notes(r#"{"version":1,"hosts":{}}"#).is_ok());
		assert!(parse_notes(r#"{"version":2,"hosts":{}}"#).is_ok());
		assert!(matches!(
			parse_notes(r#"{"version":2,"hosts":{"nas":{"connection":{"password":"x"}}}}"#),
			Err(NotesError::Malformed(_))
		));
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
	fn refuses_connection_details_that_could_inject_options() {
		let bad = [
			r#"{"hostName":"a b"}"#,
			r#"{"hostName":"x\nProxyCommand calc"}"#,
			r#"{"hostName":"-oProxyCommand=calc"}"#,
			r#"{"user":"me#x"}"#,
			r#"{"user":"-l"}"#,
			r#"{"proxyJump":"-oProxyCommand=calc"}"#,
			r#"{"proxyJump":"a b"}"#,
			r#"{"identityFile":"key\"\nProxyCommand calc"}"#,
			r#"{"port":0}"#,
		];
		for connection in bad {
			let text =
				format!(r#"{{"version":2,"hosts":{{"nas":{{"connection":{connection}}}}}}}"#);
			assert!(
				matches!(parse_notes(&text), Err(NotesError::InvalidField { .. })),
				"{connection} was accepted"
			);
		}

		let text = r#"{"version":2,"hosts":{"nas":{"connection":
			{"hostName":" 10.0.0.2 ","user":"","identityFile":"~/.ssh/my key"}}}}"#;
		let file = parse_notes(text).unwrap();
		let connection = file.hosts["nas"].connection.as_ref().unwrap();
		assert_eq!(connection.host_name.as_deref(), Some("10.0.0.2"));
		assert_eq!(connection.user, None);
		assert_eq!(connection.identity_file.as_deref(), Some("~/.ssh/my key"));

		let empty = r#"{"version":2,"hosts":{"nas":{"connection":{"user":""}}}}"#;
		assert_eq!(parse_notes(empty).unwrap().hosts["nas"].connection, None);
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
	fn previews_matched_new_and_conflicting_aliases() {
		let text = r#"{"version":2,"hosts":{"nas":{},"web1":{},"elsewhere":{},
			"pi":{"connection":{"hostName":"10.0.0.9","identityFile":"~/.ssh/pi"}}}}"#;
		let preview = preview_import(&set_of(&[CONFIG]), &parse_notes(text).unwrap());
		assert_eq!(preview.matched, vec!["nas"]);
		assert_eq!(preview.conflicting, vec!["web1"]);
		let new: Vec<_> = preview.new.iter().map(|host| host.name.as_str()).collect();
		assert_eq!(new, vec!["elsewhere", "pi"]);
		assert_eq!(preview.new[0].host_name, None);
		assert_eq!(preview.new[1].host_name.as_deref(), Some("10.0.0.9"));
		assert_eq!(preview.new[1].identity_file.as_deref(), Some("~/.ssh/pi"));
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
		let notes = parse_notes(text).unwrap();
		let changed = apply_import(&set, &notes, ListMode::Merge, false, TODAY).unwrap();
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
		let file = export_notes(&set, true);
		let changed = apply_import(&set, &file, ListMode::Overwrite, true, TODAY).unwrap();
		assert!(changed.is_empty());
	}

	const TODAY: &str = "2026-11-13";

	#[test]
	fn adds_missing_hosts_to_the_main_file() {
		let set = set_of(&[
			"Host nas\n\tHostName 10.0.0.2\n\nHost *\n\tUser me\n",
			"Host pi\n\tHostName 10.0.0.9\n",
		]);
		let text = r#"{"version":2,"hosts":{
			"nas":{"description":"Synology"},
			"web":{"description":"Work site","tags":["work"],"added":"2024-02-02",
				"connection":{"hostName":"web.example","user":"alex","port":2222,
					"identityFile":"~/.ssh/work key","proxyJump":"bastion"}},
			"old":{"description":"From a version 1 file"}}}"#;
		let notes = parse_notes(text).unwrap();
		let changed = apply_import(&set, &notes, ListMode::Merge, true, TODAY).unwrap();
		assert_eq!(changed.len(), 1);
		let (index, main) = &changed[0];
		assert_eq!(*index, 0);

		let web = main.find_host("web").unwrap();
		assert_eq!(web.option("hostname"), Some("web.example"));
		assert_eq!(web.option("user"), Some("alex"));
		assert_eq!(web.option("port"), Some("2222"));
		assert_eq!(web.option("identityfile"), Some("\"~/.ssh/work key\""));
		assert_eq!(web.option("proxyjump"), Some("bastion"));
		assert_eq!(web.meta.tags, vec!["work"]);
		assert_eq!(web.meta.added.as_deref(), Some("2024-02-02"));

		let old = main.find_host("old").unwrap();
		assert_eq!(old.option("hostname"), None);
		assert_eq!(
			old.meta.description.as_deref(),
			Some("From a version 1 file")
		);
		assert_eq!(old.meta.added.as_deref(), Some(TODAY));

		assert_eq!(
			main.find_host("nas").unwrap().meta.description.as_deref(),
			Some("Synology")
		);
		// `Host *` stays last so it still supplies defaults.
		assert!(main.to_text().trim_end().ends_with("Host *\n\tUser me"));
	}

	#[test]
	fn skips_new_hosts_when_not_asked_and_names_already_in_use() {
		let set = set_of(&[CONFIG]);
		let text = r#"{"version":2,"hosts":{
			"web1":{"connection":{"hostName":"other.example"}},
			"ghost":{"connection":{"hostName":"ghost.example"}}}}"#;
		let notes = parse_notes(text).unwrap();
		assert!(apply_import(&set, &notes, ListMode::Merge, false, TODAY)
			.unwrap()
			.is_empty());

		let changed = apply_import(&set, &notes, ListMode::Merge, true, TODAY).unwrap();
		let main = &changed[0].1;
		assert!(main.find_host("ghost").is_some());
		assert!(main
			.to_text()
			.contains("Host web1 web2\n\t# desc: shared\n"));
		assert!(!main.to_text().contains("other.example"));
	}

	#[test]
	fn connection_details_never_change_existing_hosts() {
		let set = set_of(&[CONFIG]);
		let text = r#"{"version":2,"hosts":{"nas":{"connection":{"hostName":"evil.example"}}}}"#;
		let notes = parse_notes(text).unwrap();
		let changed = apply_import(&set, &notes, ListMode::Merge, true, TODAY).unwrap();
		assert!(changed.is_empty());
	}
}
