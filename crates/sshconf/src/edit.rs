use std::fmt;

use serde::{Deserialize, Serialize};

use crate::document::{split_keyword, Block, BlockKind, Document};
use crate::meta::{is_meta_line, render_meta, HostMeta};
use crate::name::is_valid_host_name;

/// What the add and edit form sends. Empty optional fields are left out of the block.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostInput {
	pub name: String,
	pub host_name: String,
	pub user: Option<String>,
	pub port: Option<u16>,
	pub identity_file: Option<String>,
	pub proxy_jump: Option<String>,
	pub meta: HostMeta,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
	InvalidName(String),
	InvalidField(&'static str),
	AlreadyExists(String),
	NotFound(String),
}

impl fmt::Display for EditError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			EditError::InvalidName(name) => write!(f, "\"{name}\" is not a valid host name"),
			EditError::InvalidField(field) => write!(f, "The {field} field is not valid"),
			EditError::AlreadyExists(name) => write!(f, "A host called \"{name}\" already exists"),
			EditError::NotFound(name) => write!(f, "No editable host called \"{name}\""),
		}
	}
}

impl std::error::Error for EditError {}

/// The options the form manages; any other line in a block is kept as written.
const MANAGED_KEYS: [&str; 5] = ["hostname", "user", "port", "identityfile", "proxyjump"];

const DEFAULT_INDENT: &str = "    ";

impl HostInput {
	pub fn validate(&self) -> Result<(), EditError> {
		if !is_valid_host_name(&self.name) {
			return Err(EditError::InvalidName(self.name.clone()));
		}

		if !is_plain_value(&self.host_name) {
			return Err(EditError::InvalidField("address"));
		}

		if !self.user.as_deref().is_none_or(is_plain_value) {
			return Err(EditError::InvalidField("user"));
		}

		if self.port == Some(0) {
			return Err(EditError::InvalidField("port"));
		}

		let identity_ok = self
			.identity_file
			.as_deref()
			.is_none_or(|path| !path.contains(['\r', '\n', '"']));
		if !identity_ok {
			return Err(EditError::InvalidField("key file"));
		}

		if !self.proxy_jump.as_deref().is_none_or(is_plain_value) {
			return Err(EditError::InvalidField("jump host"));
		}

		if !self.meta.forwards.iter().all(|forward| forward.is_valid()) {
			return Err(EditError::InvalidField("port forward"));
		}

		Ok(())
	}

	/// Reads the form values back out of an existing block.
	pub fn from_block(block: &Block) -> HostInput {
		let owned = |key: &str| block.option(key).map(str::to_string);
		HostInput {
			name: block.patterns.first().cloned().unwrap_or_default(),
			host_name: owned("hostname").unwrap_or_default(),
			user: owned("user"),
			port: block.option("port").and_then(|port| port.parse().ok()),
			identity_file: owned("identityfile").map(|path| path.trim_matches('"').to_string()),
			proxy_jump: owned("proxyjump"),
			meta: block.meta.clone(),
		}
	}
}

/// A value written straight after a keyword: no whitespace, quotes or line breaks.
fn is_plain_value(value: &str) -> bool {
	!value
		.chars()
		.any(|c| c.is_whitespace() || c.is_control() || c == '"' || c == '#')
}

fn non_empty(value: &Option<String>) -> Option<&str> {
	value
		.as_deref()
		.map(str::trim)
		.filter(|value| !value.is_empty())
}

impl Document {
	pub fn add_host(&mut self, input: &HostInput) -> Result<(), EditError> {
		input.validate()?;
		if self.mentions_host(&input.name) {
			return Err(EditError::AlreadyExists(input.name.clone()));
		}

		let ending = self.line_ending.as_str();
		let indent = self.default_indent();
		let block = render_block(input, None, self.lines(), &indent, ending);

		// Keep a trailing `Host *` last so it still supplies defaults to every host above it.
		let trailing_wildcard = self
			.blocks
			.last()
			.filter(|block| block.kind == BlockKind::Host && block.patterns == ["*"])
			.map(|block| block.start);
		if let Some(wildcard_start) = trailing_wildcard {
			let previous_end = self.blocks.iter().rev().nth(1).map(|block| block.end);
			let mut new_lines = Vec::new();
			let at = match previous_end {
				Some(end) => {
					new_lines.push(ending.to_string());
					new_lines.extend(block);
					end
				}
				None => {
					new_lines.extend(block);
					new_lines.push(ending.to_string());
					wildcard_start
				}
			};
			self.replace_lines(at, at, new_lines);
			return Ok(());
		}

		let mut lines: Vec<String> = self.lines().to_vec();
		let count = lines.len();
		if let Some(last) = lines.last_mut() {
			if !last.ends_with('\n') {
				last.push_str(ending);
			}
		}

		let mut new_lines = Vec::new();
		let ends_blank = lines.last().is_none_or(|line| line.trim().is_empty());
		if !ends_blank {
			new_lines.push(ending.to_string());
		}
		new_lines.extend(block);
		lines.extend(new_lines);
		self.replace_lines(0, count, lines);
		Ok(())
	}

	pub fn update_host(&mut self, name: &str, input: &HostInput) -> Result<(), EditError> {
		input.validate()?;
		let block = self
			.find_host(name)
			.ok_or_else(|| EditError::NotFound(name.to_string()))?
			.clone();
		if input.name != name && self.mentions_host(&input.name) {
			return Err(EditError::AlreadyExists(input.name.clone()));
		}

		let indent = block
			.indent
			.clone()
			.unwrap_or_else(|| self.default_indent());
		let ending = self.line_ending.as_str();
		let new_lines = render_block(input, Some(&block), self.lines(), &indent, ending);
		self.replace_lines(block.start, block.end, new_lines);
		Ok(())
	}

	pub fn delete_host(&mut self, name: &str) -> Result<(), EditError> {
		let block = self
			.find_host(name)
			.ok_or_else(|| EditError::NotFound(name.to_string()))?;
		let mut start = block.start;
		let mut end = block.end;
		let lines = self.lines();
		let blank_before = start > 0 && lines[start - 1].trim().is_empty();
		let blank_after = end < lines.len() && lines[end].trim().is_empty();
		if blank_before && (blank_after || end == lines.len()) {
			// Drop one separator so deleting a block does not leave a double blank line.
			if blank_after {
				end += 1;
			}
			else {
				start -= 1;
			}
		}

		self.replace_lines(start, end, Vec::new());
		Ok(())
	}

	pub fn set_favourite(&mut self, name: &str, favourite: bool) -> Result<(), EditError> {
		let block = self
			.find_host(name)
			.ok_or_else(|| EditError::NotFound(name.to_string()))?;
		let mut input = HostInput::from_block(block);
		input.meta.favourite = favourite;
		self.update_host(name, &input)
	}

	fn default_indent(&self) -> String {
		self.blocks
			.iter()
			.find_map(|block| block.indent.clone())
			.unwrap_or_else(|| DEFAULT_INDENT.to_string())
	}
}

/// Builds a tidy block. Lines Hop does not manage are carried over from the old block unchanged.
fn render_block(
	input: &HostInput,
	existing: Option<&Block>,
	lines: &[String],
	indent: &str,
	ending: &str,
) -> Vec<String> {
	let mut body: Vec<String> = render_meta(&input.meta);
	if !input.host_name.trim().is_empty() {
		body.push(format!("HostName {}", input.host_name.trim()));
	}

	if let Some(user) = non_empty(&input.user) {
		body.push(format!("User {user}"));
	}

	if let Some(port) = input.port {
		body.push(format!("Port {port}"));
	}

	if let Some(path) = non_empty(&input.identity_file) {
		if path.contains(' ') {
			body.push(format!("IdentityFile \"{path}\""));
		}
		else {
			body.push(format!("IdentityFile {path}"));
		}
	}

	if let Some(jump) = non_empty(&input.proxy_jump) {
		body.push(format!("ProxyJump {jump}"));
	}

	let mut rendered = vec![format!("Host {}{ending}", input.name)];
	rendered.extend(
		body.into_iter()
			.map(|line| format!("{indent}{line}{ending}")),
	);
	if let Some(block) = existing {
		rendered.extend(kept_lines(block, lines, ending));
	}

	rendered
}

/// Old lines that survive an edit: other options, plain comments, and repeats of managed keys.
fn kept_lines(block: &Block, lines: &[String], ending: &str) -> Vec<String> {
	let mut seen: Vec<String> = Vec::new();
	let mut kept = Vec::new();
	for line in &lines[block.start + 1..block.end] {
		let keep = match split_keyword(line) {
			Some((key, _)) => {
				let key = key.to_ascii_lowercase();
				if MANAGED_KEYS.contains(&key.as_str()) && !seen.contains(&key) {
					seen.push(key);
					false
				}
				else {
					true
				}
			}
			None => !is_meta_line(line) && !line.trim().is_empty(),
		};
		if keep {
			let mut line = line.clone();
			if !line.ends_with('\n') {
				line.push_str(ending);
			}
			kept.push(line);
		}
	}

	kept
}
