//! The named commands the interface may call. Every argument from the page is checked here
//! before it reaches the config, a file or a program.

use std::error::Error;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use chrono::{Local, Utc};
use serde::{Deserialize, Serialize};
use sshconf::{
	is_valid_host_name, ConfigSet, Document, ExportSelection, Forward, HostInput, ImportPreview,
	ListMode, NotesFile, ResolvedHost, MAX_NOTES_FILE_BYTES,
};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_dialog::DialogExt;

use crate::history::{History, HistoryEntry};
use crate::hosts::{self, build_views, HostView};
use crate::keys::{self, AuthoriseResult, KeyFile, KeyInfo, PublicKey};
use crate::platform::{self, TerminalChoice};
use crate::settings::Settings;
use crate::status::{self, HostStatus};
use crate::tray;
use crate::tunnels::{self, TunnelState, TunnelView, Tunnels};

const SECONDS_PER_DAY: i64 = 86_400;
const MAX_HISTORY_DAYS: u32 = 3650;
pub const HOSTS_CHANGED_EVENT: &str = "hosts-changed";
pub const TUNNELS_CHANGED_EVENT: &str = "tunnels-changed";
/// How long `start_tunnel` watches a new tunnel so quick failures come back as its result.
const TUNNEL_SETTLE_CHECKS: u32 = 15;
const TUNNEL_SETTLE_STEP: Duration = Duration::from_millis(100);

pub struct AppState {
	config_path: PathBuf,
	ssh_dir: PathBuf,
	data_dir: PathBuf,
	settings_path: PathBuf,
	history: Mutex<History>,
	settings: Mutex<Settings>,
	/// Only one save at a time, so two quick edits cannot race on the same file.
	write_lock: Mutex<()>,
	pub tunnels: Tunnels,
	/// The checked notes file from the last import preview. Applying uses this copy, so the page
	/// never sends notes or file paths back to be written.
	pending_import: Mutex<Option<NotesFile>>,
}

impl AppState {
	pub fn new(app: &AppHandle) -> Result<AppState, Box<dyn Error>> {
		let data_dir = app.path().app_data_dir()?;
		fs::create_dir_all(&data_dir)?;
		let ssh_dir = platform::ssh_dir().ok_or("Could not find your home folder")?;
		let settings_path = data_dir.join("settings.json");
		let settings = match Settings::load(&settings_path) {
			Ok(settings) => settings,
			Err(error) => {
				eprintln!("hop: settings file could not be read, using defaults: {error}");
				Settings::default()
			}
		};

		let history = History::open(&data_dir.join("history.sqlite3"))?;
		if let Err(error) = prune_history(&history, &settings) {
			eprintln!("hop: {error}");
		}

		Ok(AppState {
			config_path: ssh_dir.join("config"),
			ssh_dir,
			history: Mutex::new(history),
			data_dir,
			settings_path,
			settings: Mutex::new(settings),
			write_lock: Mutex::new(()),
			tunnels: Tunnels::default(),
			pending_import: Mutex::new(None),
		})
	}

	pub fn load_config(&self) -> Result<ConfigSet, String> {
		sshconf::load(&self.config_path, &self.ssh_dir)
			.map_err(|error| format!("Could not read {}: {error}", self.config_path.display()))
	}

	pub fn history(&self) -> Result<MutexGuard<'_, History>, String> {
		self.history
			.lock()
			.map_err(|_| "History is unavailable after an earlier error".to_string())
	}

	fn settings(&self) -> Result<MutexGuard<'_, Settings>, String> {
		self.settings
			.lock()
			.map_err(|_| "Settings are unavailable after an earlier error".to_string())
	}
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostList {
	hosts: Vec<HostView>,
	warnings: Vec<String>,
	config_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostDetail {
	resolved: Option<ResolvedHost>,
	error: Option<String>,
	history: Vec<HistoryEntry>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
	settings: Settings,
	terminals: Vec<TerminalChoice>,
	config_path: String,
	data_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveHostRequest {
	/// The alias being edited, or none when adding a host.
	original: Option<String>,
	input: HostInput,
}

fn now() -> i64 {
	Utc::now().timestamp()
}

fn backup_stamp() -> String {
	Local::now().format("%Y%m%d-%H%M%S").to_string()
}

/// Applies the retention setting, so old connections do not linger longer than the user chose.
fn prune_history(history: &History, settings: &Settings) -> Result<(), String> {
	let Some(cutoff) = settings.retention_cutoff(now())
	else {
		return Ok(());
	};

	history
		.prune(cutoff)
		.map(|_| ())
		.map_err(|error| format!("Could not remove old history: {error}"))
}

fn check_name(name: &str) -> Result<(), String> {
	if is_valid_host_name(name) {
		Ok(())
	}
	else {
		Err(format!("\"{name}\" is not a valid host name"))
	}
}

/// `ssh -G` and online checks only ever run for hosts the config names. For an unknown name
/// `ssh -G` would echo it back as the address, letting the page probe any machine.
fn require_configured(set: &ConfigSet, name: &str) -> Result<(), String> {
	check_name(name)?;
	if set.file_for_host(name).is_some() {
		Ok(())
	}
	else {
		Err(format!("\"{name}\" is not a host in your SSH config"))
	}
}

fn host_list(state: &AppState) -> Result<HostList, String> {
	let set = state.load_config()?;
	let usage = state
		.history()?
		.usage()
		.map_err(|error| format!("Could not read history: {error}"))?;
	let stale_days = state.settings()?.stale_days;
	Ok(HostList {
		hosts: build_views(&set, &usage, now(), stale_days),
		warnings: set.warnings,
		config_path: state.config_path.display().to_string(),
	})
}

#[tauri::command]
pub fn list_hosts(state: State<'_, AppState>) -> Result<HostList, String> {
	host_list(&state)
}

#[tauri::command]
pub async fn host_detail(state: State<'_, AppState>, name: String) -> Result<HostDetail, String> {
	require_configured(&state.load_config()?, &name)?;
	let since = now() - 365 * SECONDS_PER_DAY;
	let history = state
		.history()?
		.entries(Some(&name), since)
		.map_err(|error| format!("Could not read history: {error}"))?;
	let resolved = tauri::async_runtime::spawn_blocking(move || {
		sshconf::resolve(&platform::ssh_program(), &name)
	})
	.await
	.map_err(|error| format!("The lookup stopped unexpectedly: {error}"))?;

	Ok(match resolved {
		Ok(resolved) => HostDetail {
			resolved: Some(resolved),
			error: None,
			history,
		},
		Err(error) => HostDetail {
			resolved: None,
			error: Some(error.to_string()),
			history,
		},
	})
}

#[tauri::command]
pub fn connect(
	app: AppHandle,
	state: State<'_, AppState>,
	name: String,
	forward: Option<usize>,
) -> Result<i64, String> {
	connect_host(&app, &state, &name, forward)
}

/// Shared by the page and the tray menu.
pub fn connect_host(
	app: &AppHandle,
	state: &AppState,
	name: &str,
	forward: Option<usize>,
) -> Result<i64, String> {
	check_name(name)?;
	let set = state.load_config()?;
	let block = set
		.files
		.iter()
		.find_map(|file| file.doc.find_host(name))
		.ok_or_else(|| format!("\"{name}\" is not a host in your SSH config"))?;

	let mut args = Vec::new();
	if let Some(index) = forward {
		let forward = block
			.meta
			.forwards
			.get(index)
			.ok_or_else(|| "That port forward no longer exists".to_string())?;
		args.push("-L".to_string());
		args.push(forward.spec());
	}
	args.push(name.to_string());

	let terminal = state.settings()?.terminal.clone();
	platform::launch_terminal(&terminal, name, &platform::ssh_program(), &args)
		.map_err(|error| format!("Could not open the terminal: {error}"))?;

	let started_at = now();
	{
		let history = state.history()?;
		history
			.record(name, started_at)
			.map_err(|error| format!("Connected, but history could not be saved: {error}"))?;
		let settings = state.settings()?.clone();
		prune_history(&history, &settings)?;
	}
	hosts_changed(app);
	Ok(started_at)
}

/// Tells the window and the tray that hosts or history changed.
pub fn hosts_changed(app: &AppHandle) {
	tray::refresh(app);
	if let Err(error) = app.emit(HOSTS_CHANGED_EVENT, ()) {
		eprintln!("hop: could not notify the window: {error}");
	}
}

fn write_doc(path: &Path, doc: &Document) -> Result<(), String> {
	sshconf::write_with_backup(path, &doc.to_text(), &backup_stamp())
		.map(|_| ())
		.map_err(|error| format!("Could not save {}: {error}", path.display()))
}

/// Loads the file that holds `name`, applies the edit and saves it with a backup.
fn edit_host_file(
	state: &AppState,
	name: &str,
	edit: impl FnOnce(&mut Document) -> Result<(), sshconf::EditError>,
) -> Result<(), String> {
	let _guard = state
		.write_lock
		.lock()
		.map_err(|_| "Saving is unavailable after an earlier error".to_string())?;
	let set = state.load_config()?;
	let file = set
		.file_for_host(name)
		.ok_or_else(|| format!("No editable host called \"{name}\""))?;
	let mut doc = file.doc.clone();
	edit(&mut doc).map_err(|error| error.to_string())?;
	write_doc(&file.path, &doc)
}

#[tauri::command]
pub fn save_host(
	app: AppHandle,
	state: State<'_, AppState>,
	request: SaveHostRequest,
) -> Result<HostList, String> {
	let SaveHostRequest {
		original,
		mut input,
	} = request;
	input.validate().map_err(|error| error.to_string())?;

	match original {
		Some(original) => {
			check_name(&original)?;
			if input.name != original && state.load_config()?.mentions_host(&input.name) {
				return Err(format!("A host called \"{}\" already exists", input.name));
			}

			edit_host_file(&state, &original, |doc| doc.update_host(&original, &input))?;
			if input.name != original {
				state
					.history()?
					.rename_host(&original, &input.name)
					.map_err(|error| format!("Saved, but history was not renamed: {error}"))?;
			}
		}
		None => {
			let _guard = state
				.write_lock
				.lock()
				.map_err(|_| "Saving is unavailable after an earlier error".to_string())?;
			let set = state.load_config()?;
			if set.mentions_host(&input.name) {
				return Err(format!("A host called \"{}\" already exists", input.name));
			}

			if input.meta.added.is_none() {
				input.meta.added = Some(Local::now().format("%Y-%m-%d").to_string());
			}

			// New hosts always go in the main config, never an included file.
			let main = &set.files[0];
			let mut doc = main.doc.clone();
			doc.add_host(&input).map_err(|error| error.to_string())?;
			write_doc(&main.path, &doc)?;
		}
	}

	hosts_changed(&app);
	host_list(&state)
}

#[tauri::command]
pub fn delete_host(
	app: AppHandle,
	state: State<'_, AppState>,
	name: String,
) -> Result<HostList, String> {
	check_name(&name)?;
	edit_host_file(&state, &name, |doc| doc.delete_host(&name))?;
	hosts_changed(&app);
	host_list(&state)
}

#[tauri::command]
pub fn set_favourite(
	app: AppHandle,
	state: State<'_, AppState>,
	name: String,
	favourite: bool,
) -> Result<HostList, String> {
	check_name(&name)?;
	edit_host_file(&state, &name, |doc| doc.set_favourite(&name, favourite))?;
	hosts_changed(&app);
	host_list(&state)
}

#[tauri::command]
pub async fn check_status(
	state: State<'_, AppState>,
	names: Vec<String>,
) -> Result<Vec<HostStatus>, String> {
	let set = state.load_config()?;
	for name in &names {
		require_configured(&set, name)?;
	}

	tauri::async_runtime::spawn_blocking(move || {
		status::check_many(&platform::ssh_program(), &names)
	})
	.await
	.map_err(|error| format!("The status check stopped unexpectedly: {error}"))
}

#[tauri::command]
pub fn get_history(
	state: State<'_, AppState>,
	name: Option<String>,
	days: u32,
) -> Result<Vec<HistoryEntry>, String> {
	if let Some(name) = &name {
		check_name(name)?;
	}

	let since = now() - i64::from(days.min(MAX_HISTORY_DAYS)) * SECONDS_PER_DAY;
	state
		.history()?
		.entries(name.as_deref(), since)
		.map_err(|error| format!("Could not read history: {error}"))
}

#[tauri::command]
pub fn clear_history(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
	state
		.history()?
		.clear()
		.map_err(|error| format!("Could not clear history: {error}"))?;
	hosts_changed(&app);
	Ok(())
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<SettingsView, String> {
	Ok(SettingsView {
		settings: state.settings()?.clone(),
		terminals: platform::terminal_choices(),
		config_path: state.config_path.display().to_string(),
		data_path: state.data_dir.display().to_string(),
	})
}

#[tauri::command]
pub fn save_settings(
	app: AppHandle,
	state: State<'_, AppState>,
	settings: Settings,
) -> Result<SettingsView, String> {
	settings.validate()?;
	let autolaunch = app.autolaunch();
	let enabled = autolaunch
		.is_enabled()
		.map_err(|error| format!("Could not read the start-up setting: {error}"))?;
	if enabled != settings.start_with_computer {
		let result = if settings.start_with_computer {
			autolaunch.enable()
		}
		else {
			autolaunch.disable()
		};
		result.map_err(|error| format!("Could not change the start-up setting: {error}"))?;
	}

	settings
		.save(&state.settings_path)
		.map_err(|error| format!("Could not save settings: {error}"))?;
	// A shorter period should take effect now, not only after the next connection.
	prune_history(&*state.history()?, &settings)?;
	*state.settings()? = settings;
	hosts_changed(&app);
	get_settings(state)
}

#[tauri::command]
pub fn list_tunnels(state: State<'_, AppState>) -> Result<Vec<TunnelView>, String> {
	state.tunnels.list()
}

/// Starts `ssh -N -L` for one saved forward, then watches it briefly so a busy port or a
/// refused login comes back as the result rather than only through the change event.
#[tauri::command]
pub async fn start_tunnel(
	app: AppHandle,
	state: State<'_, AppState>,
	name: String,
	forward: usize,
) -> Result<Vec<TunnelView>, String> {
	let set = state.load_config()?;
	require_configured(&set, &name)?;
	let saved = set
		.files
		.iter()
		.find_map(|file| file.doc.find_host(&name))
		.and_then(|block| block.meta.forwards.get(forward).cloned())
		.ok_or_else(|| "That port forward no longer exists".to_string())?;

	let ssh = platform::ssh_program();
	state.tunnels.start(&name, forward, &saved, |args| {
		tunnels::spawn_ssh(&ssh, args)
	})?;

	for _ in 0..TUNNEL_SETTLE_CHECKS {
		// Sleep on a blocking thread so the async runtime is not held up; Tauri has no timer
		// of its own and using tokio's directly would add a dependency.
		tauri::async_runtime::spawn_blocking(|| thread::sleep(TUNNEL_SETTLE_STEP))
			.await
			.map_err(|error| format!("The tunnel check stopped unexpectedly: {error}"))?;
		state.tunnels.poll()?;
		let running = state.tunnels.list()?.iter().any(|tunnel| {
			tunnel.local_port == saved.local_port && tunnel.state == TunnelState::Running
		});
		if !running {
			break;
		}
	}

	tunnels_changed(&app);
	state.tunnels.list()
}

/// Stops a running tunnel, or dismisses a failed one.
#[tauri::command]
pub fn stop_tunnel(
	app: AppHandle,
	state: State<'_, AppState>,
	local_port: u16,
) -> Result<Vec<TunnelView>, String> {
	state.tunnels.stop(local_port)?;
	tunnels_changed(&app);
	state.tunnels.list()
}

pub fn tunnels_changed(app: &AppHandle) {
	tray::refresh(app);
	if let Err(error) = app.emit(TUNNELS_CHANGED_EVENT, ()) {
		eprintln!("hop: could not notify the window: {error}");
	}
}

/// Every forward marked to open as Hop starts, with its host and position in the host's list.
pub fn auto_start_forwards(set: &ConfigSet) -> Vec<(String, usize, Forward)> {
	set.files
		.iter()
		.flat_map(|file| file.doc.blocks.iter())
		.filter(|block| block.is_editable())
		.flat_map(|block| {
			let name = block.patterns[0].clone();
			block
				.meta
				.forwards
				.iter()
				.enumerate()
				.filter(|(_, forward)| forward.auto_start)
				.map(move |(index, forward)| (name.clone(), index, forward.clone()))
		})
		.collect()
}

/// Opens the forwards marked to start with Hop. A failure is listed against its host and
/// reported without opening the window, since Hop may have been started in the tray.
pub fn start_auto_tunnels(app: &AppHandle) {
	let state = app.state::<AppState>();
	let set = match state.load_config() {
		Ok(set) => set,
		Err(error) => {
			crate::notify_error(app, &format!("Could not open tunnels at start-up: {error}"));
			return;
		}
	};

	let forwards = auto_start_forwards(&set);
	if forwards.is_empty() {
		return;
	}

	let ssh = platform::ssh_program();
	let mut failures = Vec::new();
	for (name, index, forward) in &forwards {
		let started = state
			.tunnels
			.start(name, *index, forward, |args| tunnels::spawn_ssh(&ssh, args));
		if let Err(error) = started {
			if let Err(error) = state
				.tunnels
				.record_failure(name, *index, forward, error.clone())
			{
				eprintln!("hop: could not record a tunnel failure: {error}");
			}
			failures.push(format!("{name} ({}): {error}", forward.local_port));
		}
	}

	// Tunnels that start but die soon after are caught by the watcher, which lists them too.
	tunnels_changed(app);
	if !failures.is_empty() {
		let message = format!("Some tunnels did not open: {}", failures.join("; "));
		crate::notify_error(app, &message);
	}
}

const NOTES_FILTER_NAME: &str = "Hop notes";
const NOTES_EXTENSIONS: &[&str] = &["json"];
const MAX_EXPORT_CHOICES: usize = 10_000;
/// Matches the notes file's own limit, so a tag that could be exported can also be chosen.
const MAX_TAG_CHARS: usize = 64;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotesImportPreview {
	file_name: String,
	#[serde(flatten)]
	preview: ImportPreview,
}

fn pending_import(state: &AppState) -> Result<MutexGuard<'_, Option<NotesFile>>, String> {
	state
		.pending_import
		.lock()
		.map_err(|_| "Importing is unavailable after an earlier error".to_string())
}

/// Shows the save dialog and writes the notes. Returns how many hosts were exported, or none
/// when the dialog was cancelled.
#[tauri::command]
pub async fn export_notes(
	app: AppHandle,
	selection: ExportSelection,
	include_connection: bool,
) -> Result<Option<usize>, String> {
	check_selection(&selection)?;
	tauri::async_runtime::spawn_blocking(move || {
		let state = app.state::<AppState>();
		let file = sshconf::export_selected(&state.load_config()?, &selection, include_connection);
		if file.hosts.is_empty() {
			return Err("None of the chosen hosts have anything to export".to_string());
		}

		// The file dialog runs here in Rust so the page never chooses a path to write.
		let chosen = app
			.dialog()
			.file()
			.add_filter(NOTES_FILTER_NAME, NOTES_EXTENSIONS)
			.set_file_name("hop-notes.json")
			.blocking_save_file();
		let Some(chosen) = chosen
		else {
			return Ok(None);
		};

		let path = chosen
			.into_path()
			.map_err(|error| format!("Could not use that location: {error}"))?;
		fs::write(&path, sshconf::to_json(&file))
			.map_err(|error| format!("Could not save {}: {error}", path.display()))?;
		Ok(Some(file.hosts.len()))
	})
	.await
	.map_err(|error| format!("The export stopped unexpectedly: {error}"))?
}

/// Bounds what the page may send, since the selection arrives before any dialog is shown.
fn check_selection(selection: &ExportSelection) -> Result<(), String> {
	match selection {
		ExportSelection::All => Ok(()),
		ExportSelection::Hosts { names } => {
			if names.is_empty() {
				return Err("Choose at least one host to export".to_string());
			}

			if names.len() > MAX_EXPORT_CHOICES {
				return Err("Too many hosts were chosen".to_string());
			}

			names.iter().try_for_each(|name| check_name(name))
		}
		ExportSelection::Tags { tags } => {
			if tags.is_empty() {
				return Err("Choose at least one tag to export".to_string());
			}

			if tags.len() > MAX_EXPORT_CHOICES {
				return Err("Too many tags were chosen".to_string());
			}

			if tags
				.iter()
				.any(|tag| tag.is_empty() || tag.chars().count() > MAX_TAG_CHARS)
			{
				return Err("One of the chosen tags is not valid".to_string());
			}

			Ok(())
		}
	}
}

fn read_notes_file(path: &Path) -> Result<NotesFile, String> {
	let file = fs::File::open(path)
		.map_err(|error| format!("Could not open {}: {error}", path.display()))?;
	// Read one byte past the limit, so an oversized file is refused without loading all of it.
	let mut text = String::new();
	file.take(MAX_NOTES_FILE_BYTES + 1)
		.read_to_string(&mut text)
		.map_err(|error| format!("Could not read {}: {error}", path.display()))?;
	sshconf::parse_notes(&text).map_err(|error| error.to_string())
}

/// Shows the open dialog, checks the chosen file and keeps it for `apply_notes_import`.
/// Returns none when the dialog was cancelled.
#[tauri::command]
pub async fn preview_notes_import(app: AppHandle) -> Result<Option<NotesImportPreview>, String> {
	tauri::async_runtime::spawn_blocking(move || {
		let state = app.state::<AppState>();
		*pending_import(&state)? = None;
		let chosen = app
			.dialog()
			.file()
			.add_filter(NOTES_FILTER_NAME, NOTES_EXTENSIONS)
			.blocking_pick_file();
		let Some(chosen) = chosen
		else {
			return Ok(None);
		};

		let path = chosen
			.into_path()
			.map_err(|error| format!("Could not use that file: {error}"))?;
		let notes = read_notes_file(&path)?;
		let mut preview = sshconf::preview_import(&state.load_config()?, &notes);
		hosts::flag_missing_keys(&mut preview);
		*pending_import(&state)? = Some(notes);
		let file_name = path
			.file_name()
			.map(|name| name.to_string_lossy().into_owned())
			.unwrap_or_default();
		Ok(Some(NotesImportPreview { file_name, preview }))
	})
	.await
	.map_err(|error| format!("The import stopped unexpectedly: {error}"))?
}

/// Applies the previewed file. Each config file it touches is saved once, with one backup.
/// With `add_new`, hosts that are not in the config yet are added to the main file.
#[tauri::command]
pub fn apply_notes_import(
	app: AppHandle,
	state: State<'_, AppState>,
	mode: ListMode,
	add_new: bool,
) -> Result<HostList, String> {
	let notes = pending_import(&state)?
		.take()
		.ok_or_else(|| "Choose a notes file to import first".to_string())?;
	{
		let _guard = state
			.write_lock
			.lock()
			.map_err(|_| "Saving is unavailable after an earlier error".to_string())?;
		let set = state.load_config()?;
		let today = Local::now().format("%Y-%m-%d").to_string();
		let changed = sshconf::apply_import(&set, &notes, mode, add_new, &today)
			.map_err(|error| error.to_string())?;
		for (index, doc) in &changed {
			write_doc(&set.files[*index].path, doc)?;
		}
	}

	hosts_changed(&app);
	host_list(&state)
}

#[tauri::command]
pub fn cancel_notes_import(state: State<'_, AppState>) -> Result<(), String> {
	*pending_import(&state)? = None;
	Ok(())
}

#[tauri::command]
pub async fn list_keys(state: State<'_, AppState>) -> Result<Vec<KeyFile>, String> {
	let ssh_dir = state.ssh_dir.clone();
	tauri::async_runtime::spawn_blocking(move || {
		keys::list(&ssh_dir, &platform::ssh_keygen_program())
	})
	.await
	.map_err(|error| format!("Listing keys stopped unexpectedly: {error}"))?
}

/// Shows a pasted key's fingerprint, so it can be compared with the other computer's before
/// it is added anywhere.
#[tauri::command]
pub async fn inspect_public_key(public_key: String) -> Result<KeyInfo, String> {
	let key = PublicKey::parse(&public_key)?;
	tauri::async_runtime::spawn_blocking(move || {
		keys::inspect(&platform::ssh_keygen_program(), &key)
	})
	.await
	.map_err(|error| format!("Checking the key stopped unexpectedly: {error}"))?
}

/// Opens `ssh-keygen` in a terminal to make a new ed25519 key in the SSH folder.
#[tauri::command]
pub fn create_key(state: State<'_, AppState>, name: String) -> Result<(), String> {
	let path = keys::new_key_path(&state.ssh_dir, &name)?;
	keys::ensure_ssh_dir(&state.ssh_dir)
		.map_err(|error| format!("Could not create {}: {error}", state.ssh_dir.display()))?;
	let terminal = state.settings()?.terminal.clone();
	platform::launch_terminal(
		&terminal,
		"New SSH key",
		&platform::ssh_keygen_program(),
		&keys::keygen_args(&path),
	)
	.map_err(|error| format!("Could not open the terminal: {error}"))
}

/// Adds a public key to `authorized_keys` on each chosen host, signing in with this computer's
/// existing access.
#[tauri::command]
pub async fn authorise_key(
	state: State<'_, AppState>,
	names: Vec<String>,
	public_key: String,
) -> Result<Vec<AuthoriseResult>, String> {
	if names.is_empty() {
		return Err("Choose at least one host".to_string());
	}

	if names.len() > MAX_EXPORT_CHOICES {
		return Err("Too many hosts were chosen".to_string());
	}

	let set = state.load_config()?;
	for name in &names {
		require_configured(&set, name)?;
	}

	let key = PublicKey::parse(&public_key)?;
	tauri::async_runtime::spawn_blocking(move || {
		// ssh-keygen decodes the key, so a damaged one never reaches a host.
		keys::inspect(&platform::ssh_keygen_program(), &key)?;
		Ok(keys::authorise_many(&platform::ssh_program(), &names, &key))
	})
	.await
	.map_err(|error| format!("Authorising stopped unexpectedly: {error}"))?
}

#[cfg(test)]
mod tests {
	use std::path::PathBuf;

	use sshconf::{Document, LoadedFile};

	use super::*;

	#[test]
	fn only_configured_hosts_are_resolved_or_checked() {
		let set = ConfigSet {
			files: vec![LoadedFile {
				path: PathBuf::from("config"),
				doc: Document::parse("Host nas\n\tHostName 10.0.0.2\n\nHost web1 web2\n"),
				included: false,
			}],
			warnings: Vec::new(),
		};
		assert!(require_configured(&set, "nas").is_ok());
		assert!(require_configured(&set, "example.com").is_err());
		assert!(require_configured(&set, "web1").is_err());
		assert!(require_configured(&set, "nas; calc").is_err());
	}

	#[test]
	fn only_forwards_marked_auto_start_are_opened() {
		let text = "Host nas\n\t# forward: 5001:localhost:5001 Admin\n\
			\t# forward: 8080:localhost:80 [auto] Web\n\n\
			Host web1 web2\n\t# forward: 9090:localhost:90 [auto]\n";
		let set = ConfigSet {
			files: vec![LoadedFile {
				path: PathBuf::from("config"),
				doc: Document::parse(text),
				included: false,
			}],
			warnings: Vec::new(),
		};
		let forwards = auto_start_forwards(&set);
		assert_eq!(forwards.len(), 1);
		assert_eq!(forwards[0].0, "nas");
		assert_eq!(forwards[0].1, 1);
		assert_eq!(forwards[0].2.local_port, 8080);
	}

	#[test]
	fn export_selections_are_checked() {
		assert!(check_selection(&ExportSelection::All).is_ok());
		let names = vec!["nas".to_string()];
		assert!(check_selection(&ExportSelection::Hosts { names }).is_ok());
		let names = vec!["nas; calc".to_string()];
		assert!(check_selection(&ExportSelection::Hosts { names }).is_err());
		let names = Vec::new();
		assert!(check_selection(&ExportSelection::Hosts { names }).is_err());
		let tags = vec!["x".repeat(65)];
		assert!(check_selection(&ExportSelection::Tags { tags }).is_err());
	}

	#[test]
	fn prunes_history_unless_kept_forever() {
		let history = History::in_memory().unwrap();
		history.record("nas", 0).unwrap();
		history.record("nas", now()).unwrap();

		let forever = Settings {
			history_retention_days: 0,
			..Settings::default()
		};
		prune_history(&history, &forever).unwrap();
		assert_eq!(history.entries(None, 0).unwrap().len(), 2);

		prune_history(&history, &Settings::default()).unwrap();
		assert_eq!(history.entries(None, 0).unwrap().len(), 1);
	}
}
