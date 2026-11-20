//! SSH keys: listing this computer's public keys, creating a new key and adding a public key to
//! hosts' `authorized_keys`. Hop never reads, copies or moves a private key.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;

use serde::Serialize;

/// Far above any real public key (an 8192-bit RSA key is about 1.4 KB), so a pasted file that
/// is not a key is refused before it reaches `ssh-keygen` or a server.
const MAX_PUBLIC_KEY_BYTES: usize = 16 * 1024;
const MAX_COMMENT_CHARS: usize = 256;
const MAX_KEY_NAME_CHARS: usize = 64;
const PARALLEL_AUTHORISES: usize = 8;

/// Key types OpenSSH accepts in `authorized_keys`. Requiring one as the first word also refuses
/// lines that start with options such as `command="..."`.
const KEY_TYPES: &[&str] = &[
	"ssh-ed25519",
	"ssh-rsa",
	"ecdsa-sha2-nistp256",
	"ecdsa-sha2-nistp384",
	"ecdsa-sha2-nistp521",
	"sk-ssh-ed25519@openssh.com",
	"sk-ecdsa-sha2-nistp256@openssh.com",
];

/// Runs on the host as `sh -c '<script>'`, reading the key from stdin, so nothing pasted into
/// Hop is ever part of a command line. It must not contain `'` (it is wrapped in single quotes)
/// or `!` (csh expands it even inside quotes), and is one line because csh refuses newlines in
/// quotes. A key is matched on its type and data, so the same key with another comment is not
/// added twice; the data is length-prefixed, so one key's data is never the start of another's.
const REMOTE_SCRIPT: &[&str] = &[
	"umask 077",
	"mkdir -p \"$HOME/.ssh\" || exit 1",
	"f=\"$HOME/.ssh/authorized_keys\"",
	"touch \"$f\" || exit 1",
	"read -r kind data comment || exit 1",
	"if grep -qF -e \"$kind $data\" \"$f\"; then echo hop:present; exit 0; fi",
	// Without this, a file that does not end in a newline would get the key glued to its last line.
	"if [ -s \"$f\" ] && [ -n \"$(tail -c 1 \"$f\")\" ]; then echo >> \"$f\" || exit 1; fi",
	"line=\"$kind $data\"",
	"if [ -n \"$comment\" ]; then line=\"$line $comment\"; fi",
	"printf \"%s\\n\" \"$line\" >> \"$f\" || exit 1",
	"echo hop:added",
];

const ADDED_MARKER: &str = "hop:added";
const PRESENT_MARKER: &str = "hop:present";

/// A checked, single-line public key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicKey {
	kind: String,
	data: String,
	comment: String,
}

impl PublicKey {
	pub fn parse(text: &str) -> Result<PublicKey, String> {
		if text.len() > MAX_PUBLIC_KEY_BYTES {
			return Err("That is too long to be a public key".to_string());
		}

		let text = text.trim();
		if text.is_empty() {
			return Err("Paste a public key first".to_string());
		}

		if text.contains(['\n', '\r']) {
			return Err("Paste one public key, on a single line".to_string());
		}

		if text.starts_with("-----BEGIN") || text.contains("PRIVATE KEY") {
			return Err(
				"That looks like a private key. Never share it; paste the .pub file's contents instead"
					.to_string(),
			);
		}

		let mut words = text.split_whitespace();
		let kind = words.next().unwrap_or_default();
		if !KEY_TYPES.contains(&kind) {
			return Err(
				"That is not a public key. It should start with a type such as ssh-ed25519"
					.to_string(),
			);
		}

		let data = words.next().unwrap_or_default();
		if !is_base64(data) {
			return Err("The key data is missing or damaged".to_string());
		}

		let comment = words.collect::<Vec<_>>().join(" ");
		if comment.chars().count() > MAX_COMMENT_CHARS || comment.chars().any(char::is_control) {
			return Err("The key's comment is too long or contains control characters".to_string());
		}

		Ok(PublicKey {
			kind: kind.to_string(),
			data: data.to_string(),
			comment,
		})
	}

	pub fn line(&self) -> String {
		if self.comment.is_empty() {
			format!("{} {}", self.kind, self.data)
		}
		else {
			format!("{} {} {}", self.kind, self.data, self.comment)
		}
	}
}

fn is_base64(text: &str) -> bool {
	let body = text.trim_end_matches('=');
	!body.is_empty()
		&& text.len() - body.len() <= 2
		&& body
			.chars()
			.all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/')
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyInfo {
	/// As `ssh-keygen` names it, such as `ED25519` or `RSA`.
	pub kind: String,
	pub bits: u32,
	/// `SHA256:...`, the form both `ssh-keygen -l` and OpenSSH's prompts show.
	pub fingerprint: String,
	pub comment: String,
	pub public_key: String,
}

/// Asks `ssh-keygen` for the fingerprint. It also decodes the key, so damaged data that passed
/// the simple checks is refused here.
pub fn inspect(keygen: &Path, key: &PublicKey) -> Result<KeyInfo, String> {
	let output = run_with_input(
		Command::new(keygen).args(["-l", "-E", "sha256", "-f", "-"]),
		&key.line(),
	)
	.map_err(|error| format!("Could not run ssh-keygen: {error}"))?;
	if !output.status.success() {
		return Err("ssh-keygen could not read that key; it may be damaged".to_string());
	}

	let text = String::from_utf8_lossy(&output.stdout);
	let (bits, fingerprint, kind) = parse_fingerprint(&text)
		.ok_or_else(|| "ssh-keygen gave an answer Hop did not understand".to_string())?;
	Ok(KeyInfo {
		kind,
		bits,
		fingerprint,
		comment: key.comment.clone(),
		public_key: key.line(),
	})
}

/// Reads `256 SHA256:abc... comment words (ED25519)`.
fn parse_fingerprint(text: &str) -> Option<(u32, String, String)> {
	let line = text.lines().next()?.trim();
	let (bits, rest) = line.split_once(' ')?;
	let (fingerprint, _) = rest.split_once(' ').unwrap_or((rest, ""));
	let kind = line.rsplit_once('(')?.1.strip_suffix(')')?;
	if !fingerprint.starts_with("SHA256:") {
		return None;
	}

	Some((
		bits.parse().ok()?,
		fingerprint.to_string(),
		kind.to_string(),
	))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyFile {
	pub file: String,
	pub info: Option<KeyInfo>,
	pub error: Option<String>,
}

/// Every `*.pub` file directly in the SSH folder. Only public key files are opened.
pub fn list(ssh_dir: &Path, keygen: &Path) -> Result<Vec<KeyFile>, String> {
	let entries = match fs::read_dir(ssh_dir) {
		Ok(entries) => entries,
		Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
		Err(error) => return Err(format!("Could not read {}: {error}", ssh_dir.display())),
	};

	let mut paths: Vec<PathBuf> = entries
		.filter_map(Result::ok)
		.map(|entry| entry.path())
		.filter(|path| path.extension().is_some_and(|ext| ext == "pub") && path.is_file())
		.collect();
	paths.sort();

	Ok(paths
		.iter()
		.map(|path| {
			let file = path
				.file_name()
				.map(|name| name.to_string_lossy().into_owned())
				.unwrap_or_default();
			match read_public_key(path).and_then(|key| inspect(keygen, &key)) {
				Ok(info) => KeyFile {
					file,
					info: Some(info),
					error: None,
				},
				Err(error) => KeyFile {
					file,
					info: None,
					error: Some(error),
				},
			}
		})
		.collect())
}

fn read_public_key(path: &Path) -> Result<PublicKey, String> {
	let mut text = String::new();
	fs::File::open(path)
		.and_then(|file| {
			file.take(MAX_PUBLIC_KEY_BYTES as u64 + 1)
				.read_to_string(&mut text)
		})
		.map_err(|error| format!("Could not read {}: {error}", path.display()))?;
	PublicKey::parse(&text)
}

/// A file name for a new key: the same characters as a host alias, and nothing that could be
/// read as an option or reach outside the SSH folder.
pub fn is_valid_key_name(name: &str) -> bool {
	!name.is_empty()
		&& name.chars().count() <= MAX_KEY_NAME_CHARS
		&& !name.starts_with(['-', '.'])
		&& !name.ends_with(".pub")
		&& name
			.chars()
			.all(|c| c.is_ascii_alphanumeric() || ".-_".contains(c))
}

/// Checks the name and returns the private key path `ssh-keygen` should write. Refuses a name
/// that is already used, so an existing key can never be overwritten.
pub fn new_key_path(ssh_dir: &Path, name: &str) -> Result<PathBuf, String> {
	if !is_valid_key_name(name) {
		return Err(format!(
			"\"{name}\" is not a valid key name. Use letters, digits, '.', '-' or '_'"
		));
	}

	let path = ssh_dir.join(name);
	let public = ssh_dir.join(format!("{name}.pub"));
	if path.exists() || public.exists() {
		return Err(format!("A key called \"{name}\" already exists"));
	}

	Ok(path)
}

/// A new computer may not have an SSH folder yet. OpenSSH refuses keys in a folder others can
/// write to, so on Linux it is created private.
pub fn ensure_ssh_dir(ssh_dir: &Path) -> io::Result<()> {
	let mut builder = fs::DirBuilder::new();
	builder.recursive(true);
	#[cfg(unix)]
	{
		use std::os::unix::fs::DirBuilderExt;

		builder.mode(0o700);
	}
	builder.create(ssh_dir)
}

/// `ssh-keygen` runs in a terminal so it can ask for the passphrase itself; Hop never sees it.
pub fn keygen_args(path: &Path) -> Vec<String> {
	vec![
		"-t".to_string(),
		"ed25519".to_string(),
		"-f".to_string(),
		path.to_string_lossy().into_owned(),
	]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AuthoriseOutcome {
	Added,
	AlreadyPresent,
	Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthoriseResult {
	pub name: String,
	pub outcome: AuthoriseOutcome,
	pub detail: Option<String>,
}

/// `BatchMode` turns a password or passphrase prompt into a quick failure, since nobody could
/// answer it. `-T` stops a config's `RequestTTY` from giving the script a terminal.
fn authorise_args(name: &str) -> Vec<String> {
	let mut args: Vec<String> = [
		"-T",
		"-o",
		"BatchMode=yes",
		"-o",
		"ConnectTimeout=10",
		"-o",
		"ServerAliveInterval=5",
		"-o",
		"ServerAliveCountMax=2",
	]
	.iter()
	.map(ToString::to_string)
	.collect();
	args.push(name.to_string());
	args.push(format!("sh -c '{}'", remote_script()));
	args
}

fn remote_script() -> String {
	REMOTE_SCRIPT.join("; ")
}

pub fn authorise_many(ssh: &Path, names: &[String], key: &PublicKey) -> Vec<AuthoriseResult> {
	let mut results = Vec::with_capacity(names.len());
	for chunk in names.chunks(PARALLEL_AUTHORISES) {
		thread::scope(|scope| {
			let handles: Vec<_> = chunk
				.iter()
				.map(|name| scope.spawn(move || authorise(ssh, name, key)))
				.collect();
			for (handle, name) in handles.into_iter().zip(chunk) {
				results.push(handle.join().unwrap_or_else(|_| AuthoriseResult {
					name: name.clone(),
					outcome: AuthoriseOutcome::Failed,
					detail: Some("Stopped unexpectedly".to_string()),
				}));
			}
		});
	}

	results
}

fn authorise(ssh: &Path, name: &str, key: &PublicKey) -> AuthoriseResult {
	let output = run_with_input(Command::new(ssh).args(authorise_args(name)), &key.line());
	match output {
		Ok(output) => {
			let stdout = String::from_utf8_lossy(&output.stdout);
			let stderr = String::from_utf8_lossy(&output.stderr);
			read_outcome(name, output.status.code(), &stdout, &stderr)
		}
		Err(error) => AuthoriseResult {
			name: name.to_string(),
			outcome: AuthoriseOutcome::Failed,
			detail: Some(format!("Could not run ssh: {error}")),
		},
	}
}

/// ssh exits with 255 when it could not connect or sign in; anything else came from the host.
fn read_outcome(name: &str, code: Option<i32>, stdout: &str, stderr: &str) -> AuthoriseResult {
	let result = |outcome, detail: Option<String>| AuthoriseResult {
		name: name.to_string(),
		outcome,
		detail,
	};

	if code == Some(0) {
		if stdout.lines().any(|line| line.trim() == ADDED_MARKER) {
			return result(AuthoriseOutcome::Added, None);
		}

		if stdout.lines().any(|line| line.trim() == PRESENT_MARKER) {
			return result(AuthoriseOutcome::AlreadyPresent, None);
		}
	}

	let reason = stderr
		.lines()
		.map(str::trim)
		.rfind(|line| !line.is_empty())
		.unwrap_or("no details were given");
	let detail = if code == Some(255) {
		format!("Could not sign in without a prompt: {reason}")
	}
	else {
		format!("The host refused the change (it needs a POSIX shell): {reason}")
	};
	result(AuthoriseOutcome::Failed, Some(detail))
}

/// Runs a program with no window, sends `input` and a newline on stdin, then waits for it.
fn run_with_input(command: &mut Command, input: &str) -> io::Result<Output> {
	command
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped());
	hide_console(command);
	let mut child = command.spawn()?;
	if let Some(mut stdin) = child.stdin.take() {
		// A program that exits before reading closes the pipe; its output says why, so a failed
		// write is left for the exit status to report.
		if let Err(error) = writeln!(stdin, "{input}") {
			if error.kind() != io::ErrorKind::BrokenPipe {
				return Err(error);
			}
		}
	}

	child.wait_with_output()
}

#[cfg(windows)]
fn hide_console(command: &mut Command) {
	use std::os::windows::process::CommandExt;

	const CREATE_NO_WINDOW: u32 = 0x0800_0000;
	command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console(_command: &mut Command) {}

#[cfg(test)]
mod tests {
	use super::*;

	const KEY: &str =
		"ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl";

	#[test]
	fn parses_a_public_key_and_tidies_it() {
		let key = PublicKey::parse(&format!("  {KEY}   alex@laptop  \n")).unwrap();
		assert_eq!(key.line(), format!("{KEY} alex@laptop"));
		assert_eq!(PublicKey::parse(KEY).unwrap().line(), KEY);
	}

	#[test]
	fn refuses_anything_that_is_not_one_plain_public_key() {
		let refused = [
			"".to_string(),
			"hello".to_string(),
			format!("command=\"rm -rf ~\" {KEY}"),
			format!("no-pty {KEY}"),
			format!("{KEY}\n{KEY}"),
			format!("{KEY} a\rb"),
			format!("{KEY} bell\u{7}"),
			"ssh-ed25519".to_string(),
			"ssh-ed25519 AAAA$(reboot)".to_string(),
			"ssh-ed25519 AAAA'quote".to_string(),
			"ssh-ed25519 AAAA===".to_string(),
			"-----BEGIN OPENSSH PRIVATE KEY-----".to_string(),
			format!("{KEY} {}", "x".repeat(MAX_COMMENT_CHARS + 1)),
			"x".repeat(MAX_PUBLIC_KEY_BYTES + 1),
		];
		for text in refused {
			assert!(PublicKey::parse(&text).is_err(), "accepted {text:?}");
		}
	}

	#[test]
	fn reads_ssh_keygen_fingerprints() {
		let text = "256 SHA256:+DiY3wvvV6TuJJhbpZisF/zLDA0zPMSvHdkr4UvCOqU alex@laptop (ED25519)\n";
		assert_eq!(
			parse_fingerprint(text),
			Some((
				256,
				"SHA256:+DiY3wvvV6TuJJhbpZisF/zLDA0zPMSvHdkr4UvCOqU".to_string(),
				"ED25519".to_string()
			))
		);
		let no_comment = "3072 SHA256:abc no comment (RSA)";
		assert_eq!(parse_fingerprint(no_comment).unwrap().2, "RSA");
		assert_eq!(parse_fingerprint("(stdin) is not a public key file."), None);
	}

	#[test]
	fn key_names_stay_inside_the_ssh_folder() {
		assert!(is_valid_key_name("id_ed25519"));
		assert!(is_valid_key_name("laptop-2026.key"));
		for name in [
			"",
			"../config",
			".hidden",
			"-oProxyCommand",
			"a b",
			"a/b",
			"a\\b",
			"x.pub",
		] {
			assert!(!is_valid_key_name(name), "accepted {name:?}");
		}
		assert!(!is_valid_key_name(&"a".repeat(MAX_KEY_NAME_CHARS + 1)));
	}

	#[test]
	fn never_reuses_an_existing_key_name() {
		let dir = tempfile::tempdir().unwrap();
		assert_eq!(
			new_key_path(dir.path(), "fresh").unwrap(),
			dir.path().join("fresh")
		);

		fs::write(dir.path().join("taken"), "").unwrap();
		assert!(new_key_path(dir.path(), "taken").is_err());
		fs::write(dir.path().join("half.pub"), "").unwrap();
		assert!(new_key_path(dir.path(), "half").is_err());
		assert!(new_key_path(dir.path(), "../escape").is_err());
	}

	#[test]
	fn the_remote_script_is_fixed_and_safe_to_quote() {
		let args = authorise_args("nas");
		let command = args.last().unwrap();
		let script = command
			.strip_prefix("sh -c '")
			.and_then(|rest| rest.strip_suffix('\''))
			.unwrap();
		assert!(!script.contains(['\'', '!', '\n']));
		assert_eq!(args[args.len() - 2], "nas");
		assert!(args.contains(&"BatchMode=yes".to_string()));
	}

	/// Runs the script as a host would, against a throwaway home folder.
	#[cfg(unix)]
	#[test]
	fn the_remote_script_adds_each_key_once() {
		use std::os::unix::fs::PermissionsExt;

		let home = tempfile::tempdir().unwrap();
		let run = |line: &str| {
			let output = run_with_input(
				Command::new("sh")
					.arg("-c")
					.arg(remote_script())
					.env("HOME", home.path()),
				line,
			)
			.unwrap();
			read_outcome(
				"local",
				output.status.code(),
				&String::from_utf8_lossy(&output.stdout),
				"",
			)
			.outcome
		};
		let file = home.path().join(".ssh/authorized_keys");

		assert_eq!(run(&format!("{KEY} first")), AuthoriseOutcome::Added);
		let mode = fs::metadata(&file).unwrap().permissions().mode();
		assert_eq!(mode & 0o777, 0o600);
		assert_eq!(
			run(&format!("{KEY} other comment")),
			AuthoriseOutcome::AlreadyPresent
		);

		// An existing file without a final newline keeps its last line intact.
		fs::write(&file, "ssh-rsa AAAAB3NzaC1yc2E old").unwrap();
		assert_eq!(run(KEY), AuthoriseOutcome::Added);
		assert_eq!(
			fs::read_to_string(&file).unwrap(),
			format!("ssh-rsa AAAAB3NzaC1yc2E old\n{KEY}\n")
		);
	}

	#[test]
	fn reads_what_the_host_reported() {
		let added = read_outcome("nas", Some(0), "hop:added\n", "");
		assert_eq!(added.outcome, AuthoriseOutcome::Added);

		let banner = read_outcome("nas", Some(0), "Welcome!\nhop:present\n", "");
		assert_eq!(banner.outcome, AuthoriseOutcome::AlreadyPresent);

		let denied = read_outcome(
			"nas",
			Some(255),
			"",
			"alex@nas: Permission denied (publickey).\n",
		);
		assert_eq!(denied.outcome, AuthoriseOutcome::Failed);
		assert!(denied.detail.unwrap().contains("Permission denied"));

		let no_shell = read_outcome("win", Some(1), "", "'sh' is not recognized\n");
		assert!(no_shell.detail.unwrap().contains("POSIX shell"));

		let silent = read_outcome("nas", Some(0), "", "");
		assert_eq!(silent.outcome, AuthoriseOutcome::Failed);
	}

	#[test]
	fn ssh_keygen_fingerprints_a_real_key() {
		let keygen = crate::platform::ssh_keygen_program();
		let key = PublicKey::parse(&format!("{KEY} test@example")).unwrap();
		let info = inspect(&keygen, &key).unwrap();
		assert_eq!(info.kind, "ED25519");
		assert_eq!(
			info.fingerprint,
			"SHA256:+DiY3wvvV6TuJJhbpZisF/zLDA0zPMSvHdkr4UvCOqU"
		);

		// Passes the simple checks but does not decode as a key.
		let damaged = PublicKey::parse("ssh-ed25519 AAAAbad").unwrap();
		assert!(inspect(&keygen, &damaged).is_err());
	}
}
