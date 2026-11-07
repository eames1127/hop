//! The named commands the interface may call. Every argument from the page is checked here
//! before it reaches the config, a file or a program.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use chrono::{Local, Utc};
use serde::{Deserialize, Serialize};
use sshconf::{is_valid_host_name, ConfigSet, Document, HostInput, ResolvedHost};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;

use crate::history::{History, HistoryEntry};
use crate::hosts::{build_views, HostView};
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

		Ok(AppState {
			config_path: ssh_dir.join("config"),
			ssh_dir,
			history: Mutex::new(History::open(&data_dir.join("history.sqlite3"))?),
			data_dir,
			settings_path,
			settings: Mutex::new(settings),
			write_lock: Mutex::new(()),
			tunnels: Tunnels::default(),
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
	platform::launch_terminal(&terminal, name, &args)
		.map_err(|error| format!("Could not open the terminal: {error}"))?;

	let started_at = now();
	state
		.history()?
		.record(name, started_at)
		.map_err(|error| format!("Connected, but history could not be saved: {error}"))?;
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
	if let Err(error) = app.emit(TUNNELS_CHANGED_EVENT, ()) {
		eprintln!("hop: could not notify the window: {error}");
	}
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
}
