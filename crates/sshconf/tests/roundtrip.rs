use std::fs;
use std::path::PathBuf;

use sshconf::{Document, Forward, HostInput, HostMeta};

fn fixtures() -> Vec<(String, String)> {
	let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
	let mut files: Vec<(String, String)> = fs::read_dir(dir)
		.unwrap()
		.map(|entry| entry.unwrap().path())
		.filter(|path| path.is_file())
		.map(|path| {
			let name = path.file_name().unwrap().to_string_lossy().to_string();
			(name, fs::read_to_string(&path).unwrap())
		})
		.collect();
	files.sort();
	assert!(files.len() >= 5, "fixtures are missing");
	files
}

fn fixture(name: &str) -> String {
	fixtures()
		.into_iter()
		.find(|(file, _)| file == name)
		.map(|(_, text)| text)
		.unwrap()
}

/// Lines outside the edited block must be identical, in order.
fn assert_only_block_changed(before: &str, after: &str, old_range: (usize, usize), new_len: usize) {
	let before: Vec<&str> = before.split_inclusive('\n').collect();
	let after: Vec<&str> = after.split_inclusive('\n').collect();
	assert_eq!(before[..old_range.0], after[..old_range.0]);
	assert_eq!(before[old_range.1..], after[old_range.0 + new_len..]);
}

#[test]
fn every_fixture_saves_back_byte_for_byte() {
	for (name, text) in fixtures() {
		let doc = Document::parse(&text);
		assert_eq!(doc.to_text(), text, "{name} changed on round trip");
	}
}

#[test]
fn crlf_files_keep_crlf() {
	let mut doc = Document::parse(&fixture("crlf.conf"));
	let input = HostInput {
		name: "cache".to_string(),
		host_name: "cache.example.com".to_string(),
		..HostInput::default()
	};
	doc.add_host(&input).unwrap();
	let text = doc.to_text();
	assert!(text.ends_with("Host cache\r\n\tHostName cache.example.com\r\n"));
	assert_eq!(text.matches('\n').count(), text.matches("\r\n").count());
}

#[test]
fn reads_hop_notes() {
	let doc = Document::parse(&fixture("basic.conf"));
	let nas = doc.find_host("nas").unwrap();
	assert_eq!(
		nas.meta.description.as_deref(),
		Some("Synology NAS for backups and media")
	);
	assert_eq!(nas.meta.tags, vec!["home", "storage"]);
	assert!(nas.meta.favourite);
	assert_eq!(nas.meta.forwards[0].label, "Admin UI");
	assert_eq!(nas.meta.added.as_deref(), Some("2025-03-14"));
	assert_eq!(doc.includes(), vec!["conf.d/*.conf"]);
}

#[test]
fn complex_blocks_are_read_only() {
	let doc = Document::parse(&fixture("match-and-wildcards.conf"));
	assert!(doc.find_host("build").is_some());
	assert!(doc.find_host("web1").is_none());
	assert!(doc.mentions_host("web1"));
	let editable = doc
		.blocks
		.iter()
		.filter(|block| block.is_editable())
		.count();
	assert_eq!(editable, 1);
}

#[test]
fn update_changes_only_that_block() {
	let text = fixture("basic.conf");
	let mut doc = Document::parse(&text);
	let pi = doc.find_host("pi").unwrap().clone();
	let mut input = HostInput::from_block(&pi);
	input.user = Some("admin".to_string());
	input.meta.description = Some("Shed Pi".to_string());
	doc.update_host("pi", &input).unwrap();

	let after = doc.to_text();
	let new_pi = doc.find_host("pi").unwrap();
	assert_eq!(new_pi.option("user"), Some("admin"));
	assert_eq!(new_pi.option("port"), Some("2222"));
	assert_eq!(new_pi.indent.as_deref(), Some("    "));
	assert_only_block_changed(&text, &after, (pi.start, pi.end), new_pi.end - new_pi.start);
}

#[test]
fn update_keeps_unmanaged_lines() {
	let text = fixture("match-and-wildcards.conf");
	let mut doc = Document::parse(&text);
	let build = doc.find_host("build").unwrap().clone();
	let mut input = HostInput::from_block(&build);
	input.port = Some(2200);
	doc.update_host("build", &input).unwrap();

	let after = doc.to_text();
	assert!(after.starts_with(
		"Host build\n\tHostName 10.0.0.5\n\tPort 2200\n\t# a plain note\n\tLocalForward 8080 localhost:80\n"
	));
	let new_build = doc.find_host("build").unwrap();
	assert_only_block_changed(&text, &after, (build.start, build.end), new_build.end);
}

#[test]
fn unchanged_input_is_a_no_op_for_tidy_blocks() {
	let text = fixture("crlf.conf");
	let mut doc = Document::parse(&text);
	let web = doc.find_host("web").unwrap().clone();
	doc.update_host("web", &HostInput::from_block(&web))
		.unwrap();
	assert_eq!(doc.to_text(), text);
}

#[test]
fn add_goes_before_trailing_wildcard() {
	let text = fixture("basic.conf");
	let mut doc = Document::parse(&text);
	let input = HostInput {
		name: "router".to_string(),
		host_name: "192.168.1.1".to_string(),
		user: Some("admin".to_string()),
		meta: HostMeta {
			tags: vec!["home".to_string()],
			forwards: vec![Forward {
				local_port: 8443,
				remote_host: "localhost".to_string(),
				remote_port: 443,
				label: "Web".to_string(),
				auto_start: false,
			}],
			..HostMeta::default()
		},
		..HostInput::default()
	};
	doc.add_host(&input).unwrap();

	let after = doc.to_text();
	// The new block takes the first indented block's style (tabs, from `nas`).
	let expected = "    Port 2222\n\nHost router\n\t# tags: home\n\t\
		# forward: 8443:localhost:443 Web\n\tHostName 192.168.1.1\n\tUser admin\n\nHost *\n";
	assert!(after.contains(expected), "{after}");
	assert_eq!(doc.blocks.last().unwrap().patterns, vec!["*"]);
}

#[test]
fn add_appends_and_fixes_missing_final_newline() {
	let text = fixture("odd-comments.conf");
	let mut doc = Document::parse(&text);
	let input = HostInput {
		name: "new".to_string(),
		host_name: "new.example.com".to_string(),
		..HostInput::default()
	};
	doc.add_host(&input).unwrap();
	let after = doc.to_text();
	assert!(after.starts_with(&format!("{text}\n")));
	// Indent comes from the first indented block, `spaced`, which uses eight spaces.
	let expected = "last.example.com\n\nHost new\n        HostName new.example.com\n";
	assert!(after.ends_with(expected), "{after}");
}

#[test]
fn add_to_empty_file() {
	let mut doc = Document::parse("");
	let input = HostInput {
		name: "first".to_string(),
		host_name: "first.example.com".to_string(),
		..HostInput::default()
	};
	doc.add_host(&input).unwrap();
	assert_eq!(doc.blocks.len(), 1);
	assert!(doc.to_text().starts_with("Host first"));
}

#[test]
fn delete_removes_block_and_one_separator() {
	let text = fixture("basic.conf");
	let mut doc = Document::parse(&text);
	doc.delete_host("pi").unwrap();
	let after = doc.to_text();
	assert!(!after.contains("Host pi"));
	assert!(after.contains("# Raspberry Pi in the shed\n\nHost *"));
	assert!(!after.contains("\n\n\n"));
}

#[test]
fn rename_and_duplicates() {
	let mut doc = Document::parse(&fixture("basic.conf"));
	let mut input = HostInput::from_block(doc.find_host("pi").unwrap());
	input.name = "nas".to_string();
	assert!(doc.update_host("pi", &input).is_err());
	input.name = "shed".to_string();
	doc.update_host("pi", &input).unwrap();
	assert!(doc.find_host("shed").is_some());
	assert!(doc.find_host("pi").is_none());
}

#[test]
fn favourite_toggle() {
	let mut doc = Document::parse(&fixture("basic.conf"));
	doc.set_favourite("pi", true).unwrap();
	assert!(doc.find_host("pi").unwrap().meta.favourite);
	doc.set_favourite("nas", false).unwrap();
	assert!(!doc.find_host("nas").unwrap().meta.favourite);
	assert!(!doc.to_text().contains("# fav: false"));
}

#[test]
fn edits_refuse_injection() {
	let mut doc = Document::parse("");
	let bad_names = ["nas; rm -rf ~", "-oProxyCommand=calc", "a b", "*"];
	for name in bad_names {
		let input = HostInput {
			name: name.to_string(),
			..HostInput::default()
		};
		assert!(doc.add_host(&input).is_err(), "{name} was accepted");
	}

	let input = HostInput {
		name: "ok".to_string(),
		host_name: "example.com\nProxyCommand calc".to_string(),
		..HostInput::default()
	};
	assert!(doc.add_host(&input).is_err());

	let input = HostInput {
		name: "ok".to_string(),
		user: Some("root\n\tProxyCommand calc".to_string()),
		..HostInput::default()
	};
	assert!(doc.add_host(&input).is_err());
	assert_eq!(doc.to_text(), "");
}

#[test]
fn auto_start_forwards_round_trip_and_old_ones_are_untouched() {
	let text = "Host nas\n\
		\t# forward: 5001:localhost:5001 Admin UI\n\
		\t# forward: 8080:localhost:80 [auto] Web\n\
		\tHostName nas.local\n";
	let mut doc = Document::parse(text);
	let nas = doc.find_host("nas").unwrap().clone();
	let forwards = &nas.meta.forwards;
	assert!(!forwards[0].auto_start);
	assert!(forwards[1].auto_start);
	assert_eq!(forwards[1].label, "Web");

	doc.update_host("nas", &HostInput::from_block(&nas))
		.unwrap();
	assert_eq!(doc.to_text(), text);

	let mut input = HostInput::from_block(&nas);
	input.meta.forwards[0].auto_start = true;
	input.meta.forwards[1].auto_start = false;
	doc.update_host("nas", &input).unwrap();
	let after = doc.to_text();
	assert!(
		after.contains("# forward: 5001:localhost:5001 [auto] Admin UI\n"),
		"{after}"
	);
	assert!(
		after.contains("# forward: 8080:localhost:80 Web\n"),
		"{after}"
	);
}
