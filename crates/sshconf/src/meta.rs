use serde::{Deserialize, Serialize};

use crate::name::is_valid_host_name;

/// Hop's own notes about a host, stored as `# key: value` comments inside its block.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostMeta {
	pub description: Option<String>,
	pub tags: Vec<String>,
	pub favourite: bool,
	pub forwards: Vec<Forward>,
	pub added: Option<String>,
}

/// A saved local port forward, written as `# forward: 5001:localhost:5001 Admin UI`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Forward {
	pub local_port: u16,
	pub remote_host: String,
	pub remote_port: u16,
	pub label: String,
}

impl Forward {
	pub fn parse(text: &str) -> Option<Forward> {
		let text = text.trim();
		let (spec, label) = match text.split_once(char::is_whitespace) {
			Some((spec, label)) => (spec, label.trim()),
			None => (text, ""),
		};
		let mut parts = spec.split(':');
		let local_port = parts.next()?.parse().ok()?;
		let remote_host = parts.next()?.to_string();
		let remote_port = parts.next()?.parse().ok()?;
		if parts.next().is_some() {
			return None;
		}

		let forward = Forward {
			local_port,
			remote_host,
			remote_port,
			label: label.to_string(),
		};
		if forward.is_valid() {
			Some(forward)
		}
		else {
			None
		}
	}

	pub fn is_valid(&self) -> bool {
		self.local_port != 0
			&& self.remote_port != 0
			&& is_valid_host_name(&self.remote_host)
			&& !self.label.contains(['\r', '\n'])
	}

	/// The `-L` argument for `ssh`.
	pub fn spec(&self) -> String {
		format!(
			"{}:{}:{}",
			self.local_port, self.remote_host, self.remote_port
		)
	}

	fn to_comment_value(&self) -> String {
		let label = self.label.trim();
		if label.is_empty() {
			self.spec()
		}
		else {
			format!("{} {}", self.spec(), label)
		}
	}
}

/// Reads one comment line as a Hop note and applies it. Returns false for ordinary comments.
pub(crate) fn apply_meta_line(meta: &mut HostMeta, line: &str) -> bool {
	let Some((key, value)) = split_meta_line(line)
	else {
		return false;
	};

	match key {
		"desc" => {
			meta.description = non_empty(value);
		}
		"tags" => {
			meta.tags = split_tags(value);
		}
		"fav" => {
			meta.favourite = value.eq_ignore_ascii_case("true");
		}
		"forward" => match Forward::parse(value) {
			Some(forward) => meta.forwards.push(forward),
			None => return false,
		},
		"added" => {
			meta.added = non_empty(value);
		}
		_ => return false,
	}

	true
}

pub(crate) fn is_meta_line(line: &str) -> bool {
	split_meta_line(line).is_some()
}

fn split_meta_line(line: &str) -> Option<(&str, &str)> {
	let comment = line.trim().strip_prefix('#')?.trim_start();
	let (key, value) = comment.split_once(':')?;
	let key = key.trim();
	if !matches!(key, "desc" | "tags" | "fav" | "forward" | "added") {
		return None;
	}

	Some((key, value.trim()))
}

fn non_empty(value: &str) -> Option<String> {
	if value.is_empty() {
		None
	}
	else {
		Some(value.to_string())
	}
}

pub(crate) fn split_tags(value: &str) -> Vec<String> {
	value
		.split(',')
		.map(str::trim)
		.filter(|tag| !tag.is_empty())
		.map(str::to_string)
		.collect()
}

/// Renders the notes as comment lines, without indentation or line endings.
pub(crate) fn render_meta(meta: &HostMeta) -> Vec<String> {
	let mut lines = Vec::new();
	if let Some(description) = meta.description.as_deref().map(single_line) {
		if !description.is_empty() {
			lines.push(format!("# desc: {description}"));
		}
	}

	let tags: Vec<String> = meta
		.tags
		.iter()
		.map(|tag| single_line(tag).replace(',', " "))
		.map(|tag| tag.trim().to_string())
		.filter(|tag| !tag.is_empty())
		.collect();
	if !tags.is_empty() {
		lines.push(format!("# tags: {}", tags.join(", ")));
	}

	if meta.favourite {
		lines.push("# fav: true".to_string());
	}

	for forward in &meta.forwards {
		lines.push(format!("# forward: {}", forward.to_comment_value()));
	}

	if let Some(added) = meta.added.as_deref().map(single_line) {
		if !added.is_empty() {
			lines.push(format!("# added: {added}"));
		}
	}

	lines
}

/// Notes must stay on one line, or they could inject options into the config.
pub(crate) fn single_line(text: &str) -> String {
	text.replace(['\r', '\n'], " ").trim().to_string()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parses_forward_with_label() {
		let forward = Forward::parse("5001:localhost:5001 Admin UI").unwrap();
		assert_eq!(forward.local_port, 5001);
		assert_eq!(forward.remote_host, "localhost");
		assert_eq!(forward.label, "Admin UI");
		assert_eq!(forward.spec(), "5001:localhost:5001");
	}

	#[test]
	fn refuses_bad_forwards() {
		assert!(Forward::parse("abc:localhost:5001").is_none());
		assert!(Forward::parse("5001:local;host:5001").is_none());
		assert!(Forward::parse("5001:localhost").is_none());
		assert!(Forward::parse("0:localhost:22").is_none());
	}

	#[test]
	fn reads_and_renders_meta() {
		let mut meta = HostMeta::default();
		assert!(apply_meta_line(&mut meta, "\t# desc: Synology NAS"));
		assert!(apply_meta_line(&mut meta, "# tags: home, storage"));
		assert!(apply_meta_line(&mut meta, "# fav: true"));
		assert!(!apply_meta_line(&mut meta, "# just a note"));
		assert_eq!(meta.tags, vec!["home", "storage"]);
		assert!(meta.favourite);
		let rendered = render_meta(&meta);
		assert_eq!(rendered[0], "# desc: Synology NAS");
		assert_eq!(rendered[1], "# tags: home, storage");
	}

	#[test]
	fn newlines_in_notes_cannot_inject_options() {
		let meta = HostMeta {
			description: Some("nice\nProxyCommand calc".to_string()),
			..HostMeta::default()
		};
		assert_eq!(render_meta(&meta), vec!["# desc: nice ProxyCommand calc"]);
	}
}
