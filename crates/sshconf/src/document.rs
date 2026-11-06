use serde::Serialize;

use crate::meta::{apply_meta_line, HostMeta};
use crate::name::is_valid_host_name;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
	Lf,
	CrLf,
}

impl LineEnding {
	/// Keeps the file's own style; a new, empty file uses the platform's usual ending.
	pub fn detect(text: &str) -> LineEnding {
		let new_windows_file = text.is_empty() && cfg!(windows);
		if text.contains("\r\n") || new_windows_file {
			LineEnding::CrLf
		}
		else {
			LineEnding::Lf
		}
	}

	pub fn as_str(self) -> &'static str {
		match self {
			LineEnding::Lf => "\n",
			LineEnding::CrLf => "\r\n",
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BlockKind {
	Host,
	Match,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigOption {
	pub key: String,
	pub value: String,
	pub line: usize,
}

/// A `Host` or `Match` block and the line range it owns.
#[derive(Debug, Clone)]
pub struct Block {
	pub kind: BlockKind,
	/// Host patterns, or the raw criteria of a `Match` line as one entry.
	pub patterns: Vec<String>,
	pub options: Vec<ConfigOption>,
	pub meta: HostMeta,
	/// First line (the `Host` or `Match` line).
	pub start: usize,
	/// One past the last line the block owns.
	pub end: usize,
	pub indent: Option<String>,
}

impl Block {
	/// Only a `Host` block with one plain alias may be rewritten by Hop.
	pub fn is_editable(&self) -> bool {
		self.kind == BlockKind::Host
			&& self.patterns.len() == 1
			&& is_valid_host_name(&self.patterns[0])
	}

	pub fn option(&self, key: &str) -> Option<&str> {
		self.options
			.iter()
			.find(|option| option.key.eq_ignore_ascii_case(key))
			.map(|option| option.value.as_str())
	}

	pub fn options_named<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a str> + 'a {
		self.options
			.iter()
			.filter(move |option| option.key.eq_ignore_ascii_case(key))
			.map(|option| option.value.as_str())
	}

	pub fn label(&self) -> String {
		match self.kind {
			BlockKind::Host => self.patterns.join(" "),
			BlockKind::Match => format!("Match {}", self.patterns.join(" ")),
		}
	}
}

/// A parsed config file that can be written back byte for byte.
#[derive(Debug, Clone)]
pub struct Document {
	lines: Vec<String>,
	pub blocks: Vec<Block>,
	/// Options before the first block, such as global settings and `Include`.
	pub preamble: Vec<ConfigOption>,
	pub line_ending: LineEnding,
}

impl Document {
	pub fn parse(text: &str) -> Document {
		let lines: Vec<String> = text.split_inclusive('\n').map(str::to_string).collect();
		let line_ending = LineEnding::detect(text);
		let starts: Vec<usize> = lines
			.iter()
			.enumerate()
			.filter(|(_, line)| block_kind(line).is_some())
			.map(|(index, _)| index)
			.collect();

		let preamble_end = starts.first().copied().unwrap_or(lines.len());
		let preamble = (0..preamble_end)
			.filter_map(|index| option_at(&lines, index))
			.collect();

		let mut blocks = Vec::new();
		for (position, &start) in starts.iter().enumerate() {
			let limit = starts.get(position + 1).copied().unwrap_or(lines.len());
			blocks.push(parse_block(&lines, start, limit));
		}

		Document {
			lines,
			blocks,
			preamble,
			line_ending,
		}
	}

	pub fn to_text(&self) -> String {
		self.lines.concat()
	}

	pub fn lines(&self) -> &[String] {
		&self.lines
	}

	/// Every `Include` argument in the file, in order.
	pub fn includes(&self) -> Vec<String> {
		let block_options = self.blocks.iter().flat_map(|block| block.options.iter());
		let mut options: Vec<&ConfigOption> = self.preamble.iter().chain(block_options).collect();
		options.sort_by_key(|option| option.line);
		options
			.into_iter()
			.filter(|option| option.key.eq_ignore_ascii_case("include"))
			.flat_map(|option| split_args(&option.value))
			.collect()
	}

	/// The editable block for this alias, if any.
	pub fn find_host(&self, name: &str) -> Option<&Block> {
		self.blocks
			.iter()
			.find(|block| block.is_editable() && block.patterns[0] == name)
	}

	/// True if any `Host` line names this alias exactly, editable or not.
	pub fn mentions_host(&self, name: &str) -> bool {
		self.blocks.iter().any(|block| {
			block.kind == BlockKind::Host && block.patterns.iter().any(|pattern| pattern == name)
		})
	}

	pub(crate) fn replace_lines(&mut self, start: usize, end: usize, new_lines: Vec<String>) {
		self.lines.splice(start..end, new_lines);
		*self = Document::parse(&self.to_text());
	}
}

fn parse_block(lines: &[String], start: usize, limit: usize) -> Block {
	let value = split_keyword(&lines[start])
		.map(|(_, value)| value)
		.unwrap_or_default();
	let kind = block_kind(&lines[start]).unwrap_or(BlockKind::Host);
	let patterns = match kind {
		BlockKind::Host => split_args(&value),
		BlockKind::Match => vec![value],
	};

	// Blank lines and unindented comments at the end sit between blocks, not inside this one.
	let mut end = limit;
	while end > start + 1 && is_gap_line(&lines[end - 1]) {
		end -= 1;
	}

	let mut options = Vec::new();
	let mut meta = HostMeta::default();
	let mut indent = None;
	for (index, line) in lines.iter().enumerate().take(end).skip(start + 1) {
		if let Some(option) = option_at(lines, index) {
			if indent.is_none() {
				indent = Some(leading_whitespace(line));
			}
			options.push(option);
		}
		else {
			apply_meta_line(&mut meta, line);
		}
	}

	Block {
		kind,
		patterns,
		options,
		meta,
		start,
		end,
		indent: indent.filter(|indent| !indent.is_empty()),
	}
}

fn is_gap_line(line: &str) -> bool {
	let trimmed = line.trim();
	if trimmed.is_empty() {
		return true;
	}

	trimmed.starts_with('#') && !line.starts_with(char::is_whitespace)
}

fn option_at(lines: &[String], index: usize) -> Option<ConfigOption> {
	let (key, value) = split_keyword(&lines[index])?;
	Some(ConfigOption {
		key,
		value,
		line: index,
	})
}

fn block_kind(line: &str) -> Option<BlockKind> {
	let (key, _) = split_keyword(line)?;
	if key.eq_ignore_ascii_case("host") {
		Some(BlockKind::Host)
	}
	else if key.eq_ignore_ascii_case("match") {
		Some(BlockKind::Match)
	}
	else {
		None
	}
}

/// Splits `Keyword value` or `Keyword=value`. Comments and blank lines give `None`.
pub(crate) fn split_keyword(line: &str) -> Option<(String, String)> {
	let trimmed = line.trim();
	if trimmed.is_empty() || trimmed.starts_with('#') {
		return None;
	}

	let end = trimmed
		.find(|c: char| c.is_whitespace() || c == '=')
		.unwrap_or(trimmed.len());
	let rest = trimmed[end..].trim_start();
	let rest = rest.strip_prefix('=').unwrap_or(rest).trim();
	Some((trimmed[..end].to_string(), rest.to_string()))
}

/// Splits arguments on whitespace, keeping double-quoted parts together.
pub(crate) fn split_args(value: &str) -> Vec<String> {
	let mut args = Vec::new();
	let mut current = String::new();
	let mut quoted = false;
	for c in value.chars() {
		if c == '"' {
			quoted = !quoted;
		}
		else if c.is_whitespace() && !quoted {
			if !current.is_empty() {
				args.push(std::mem::take(&mut current));
			}
		}
		else {
			current.push(c);
		}
	}

	if !current.is_empty() {
		args.push(current);
	}

	args
}

fn leading_whitespace(line: &str) -> String {
	line.chars()
		.take_while(|c| *c == ' ' || *c == '\t')
		.collect()
}

#[cfg(test)]
mod tests {
	use super::*;

	const SAMPLE: &str =
		"Include conf.d/*\n\n# NAS\nHost nas\n\t# desc: NAS\n\tHostName 10.0.0.2\n\n\
		Host web db\n    User admin\n\nMatch host *.lan\n  User pi\n";

	#[test]
	fn keeps_every_byte() {
		assert_eq!(Document::parse(SAMPLE).to_text(), SAMPLE);
	}

	#[test]
	fn finds_blocks_and_ranges() {
		let doc = Document::parse(SAMPLE);
		assert_eq!(doc.blocks.len(), 3);
		let nas = doc.find_host("nas").unwrap();
		assert_eq!(nas.start, 3);
		assert_eq!(nas.end, 6);
		assert_eq!(nas.option("hostname"), Some("10.0.0.2"));
		assert_eq!(nas.meta.description.as_deref(), Some("NAS"));
		assert_eq!(nas.indent.as_deref(), Some("\t"));
		assert!(!doc.blocks[1].is_editable());
		assert_eq!(doc.blocks[2].kind, BlockKind::Match);
		assert_eq!(doc.includes(), vec!["conf.d/*"]);
	}

	#[test]
	fn splits_equals_and_quotes() {
		assert_eq!(
			split_keyword("  Port=2222"),
			Some(("Port".to_string(), "2222".to_string()))
		);
		assert_eq!(split_args("a \"b c\" d"), vec!["a", "b c", "d"]);
	}
}
