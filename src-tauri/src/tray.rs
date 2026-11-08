//! Tray menu with favourites, recent hosts and open tunnels, so common jobs need no window.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Wry};

use crate::commands::{connect_host, tunnels_changed, AppState};
use crate::tunnels::{TunnelState, TunnelView};

const TRAY_ID: &str = "main";
const RECENT_COUNT: usize = 5;
const FAVOURITE_PREFIX: &str = "fav:";
const RECENT_PREFIX: &str = "recent:";
const TUNNEL_PREFIX: &str = "tunnel:";

pub fn create(app: &AppHandle) -> tauri::Result<()> {
	let menu = build_menu(app)?;
	let mut builder = TrayIconBuilder::with_id(TRAY_ID)
		.tooltip("Hop")
		.menu(&menu)
		.show_menu_on_left_click(true)
		.on_menu_event(|app, event| handle_menu(app, event.id().as_ref()));
	if let Some(icon) = app.default_window_icon() {
		builder = builder.icon(icon.clone());
	}

	builder.build(app)?;
	Ok(())
}

/// Rebuilds the menu after hosts, favourites or history change.
pub fn refresh(app: &AppHandle) {
	let Some(tray) = app.tray_by_id(TRAY_ID)
	else {
		return;
	};

	match build_menu(app) {
		Ok(menu) => {
			if let Err(error) = tray.set_menu(Some(menu)) {
				eprintln!("hop: could not update the tray menu: {error}");
			}
		}
		Err(error) => eprintln!("hop: could not build the tray menu: {error}"),
	}
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
	let (favourites, recent) = menu_hosts(app);
	let menu = Menu::new(app)?;
	menu.append(&MenuItem::with_id(
		app,
		"open",
		"Open Hop",
		true,
		None::<&str>,
	)?)?;
	append_section(app, &menu, "Favourites", FAVOURITE_PREFIX, &favourites)?;
	append_section(app, &menu, "Recent", RECENT_PREFIX, &recent)?;
	append_tunnels(app, &menu)?;
	menu.append(&PredefinedMenuItem::separator(app)?)?;
	menu.append(&MenuItem::with_id(
		app,
		"quit",
		"Quit Hop",
		true,
		None::<&str>,
	)?)?;
	Ok(menu)
}

fn append_section(
	app: &AppHandle,
	menu: &Menu<Wry>,
	title: &str,
	prefix: &str,
	names: &[String],
) -> tauri::Result<()> {
	if names.is_empty() {
		return Ok(());
	}

	menu.append(&PredefinedMenuItem::separator(app)?)?;
	let heading_id = format!("{prefix}heading");
	menu.append(&MenuItem::with_id(
		app,
		heading_id,
		title,
		false,
		None::<&str>,
	)?)?;
	for name in names {
		let id = format!("{prefix}{name}");
		menu.append(&MenuItem::with_id(app, id, name, true, None::<&str>)?)?;
	}

	Ok(())
}

/// One item per tunnel; choosing it stops a running tunnel or clears a failed one.
fn append_tunnels(app: &AppHandle, menu: &Menu<Wry>) -> tauri::Result<()> {
	let tunnels = match app.state::<AppState>().tunnels.list() {
		Ok(tunnels) => tunnels,
		Err(error) => {
			eprintln!("hop: tray could not list tunnels: {error}");
			return Ok(());
		}
	};
	if tunnels.is_empty() {
		return Ok(());
	}

	menu.append(&PredefinedMenuItem::separator(app)?)?;
	menu.append(&MenuItem::with_id(
		app,
		format!("{TUNNEL_PREFIX}heading"),
		"Tunnels",
		false,
		None::<&str>,
	)?)?;
	for tunnel in &tunnels {
		let id = format!("{TUNNEL_PREFIX}{}", tunnel.local_port);
		menu.append(&MenuItem::with_id(
			app,
			id,
			tunnel_item_text(tunnel),
			true,
			None::<&str>,
		)?)?;
	}

	Ok(())
}

fn tunnel_item_text(tunnel: &TunnelView) -> String {
	let target = if tunnel.label.is_empty() {
		format!("{} localhost:{}", tunnel.host, tunnel.local_port)
	}
	else {
		format!(
			"{} {} (localhost:{})",
			tunnel.host, tunnel.label, tunnel.local_port
		)
	};
	match tunnel.state {
		TunnelState::Running => format!("Stop {target}"),
		TunnelState::Failed => format!("Clear failed {target}"),
	}
}

fn stop_tunnel(app: &AppHandle, id: &str) {
	let Ok(local_port) = id.parse::<u16>()
	else {
		return;
	};

	match app.state::<AppState>().tunnels.stop(local_port) {
		Ok(()) => tunnels_changed(app),
		Err(error) => crate::show_error(app, &error),
	}
}

/// Favourite aliases and recently used ones that are still in the config.
fn menu_hosts(app: &AppHandle) -> (Vec<String>, Vec<String>) {
	let state = app.state::<AppState>();
	let set = match state.load_config() {
		Ok(set) => set,
		Err(error) => {
			eprintln!("hop: tray could not read the config: {error}");
			return (Vec::new(), Vec::new());
		}
	};

	let blocks = set
		.files
		.iter()
		.flat_map(|file| file.doc.blocks.iter())
		.filter(|block| block.is_editable());
	let mut favourites = Vec::new();
	let mut known = Vec::new();
	for block in blocks {
		let name = block.patterns[0].clone();
		if block.meta.favourite {
			favourites.push(name.clone());
		}
		known.push(name);
	}
	favourites.sort();

	let recent = match state.history().and_then(|history| {
		history
			.recent_hosts(RECENT_COUNT * 2)
			.map_err(|error| error.to_string())
	}) {
		Ok(recent) => recent,
		Err(error) => {
			eprintln!("hop: tray could not read history: {error}");
			Vec::new()
		}
	};
	let recent = recent
		.into_iter()
		.filter(|name| known.contains(name))
		.take(RECENT_COUNT)
		.collect();

	(favourites, recent)
}

fn handle_menu(app: &AppHandle, id: &str) {
	match id {
		"open" => crate::show_main(app, false),
		"quit" => app.exit(0),
		_ => {
			if let Some(port) = id.strip_prefix(TUNNEL_PREFIX) {
				stop_tunnel(app, port);
				return;
			}

			let name = id
				.strip_prefix(FAVOURITE_PREFIX)
				.or_else(|| id.strip_prefix(RECENT_PREFIX));
			if let Some(name) = name {
				let state = app.state::<AppState>();
				if let Err(error) = connect_host(app, &state, name, None) {
					eprintln!("hop: could not connect to {name}: {error}");
					crate::show_error(app, &error);
				}
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn tunnel(label: &str, state: TunnelState) -> TunnelView {
		TunnelView {
			host: "nas".to_string(),
			forward: 0,
			local_port: 8080,
			remote_host: "localhost".to_string(),
			remote_port: 80,
			label: label.to_string(),
			state,
			error: None,
		}
	}

	#[test]
	fn tunnel_items_say_what_choosing_them_does() {
		assert_eq!(
			tunnel_item_text(&tunnel("", TunnelState::Running)),
			"Stop nas localhost:8080"
		);
		assert_eq!(
			tunnel_item_text(&tunnel("Admin", TunnelState::Running)),
			"Stop nas Admin (localhost:8080)"
		);
		assert_eq!(
			tunnel_item_text(&tunnel("", TunnelState::Failed)),
			"Clear failed nas localhost:8080"
		);
	}
}
