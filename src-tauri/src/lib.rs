mod commands;
mod history;
mod hosts;
mod platform;
mod settings;
mod status;
mod tray;
mod tunnels;

use std::thread;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

/// Passed by the autostart entry so Hop starts in the tray.
const MINIMISED_FLAG: &str = "--minimised";
/// `hop --quick` opens the window with search focused. KDE users bind a custom shortcut to it,
/// because global shortcuts are unreliable under Wayland.
const QUICK_FLAG: &str = "--quick";
const QUICK_LAUNCH_EVENT: &str = "quick-launch";
const ERROR_EVENT: &str = "hop-error";

pub fn run() {
	let quick_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyH);
	let shortcut_plugin = tauri_plugin_global_shortcut::Builder::new()
		.with_handler(move |app, shortcut, event| {
			if event.state() == ShortcutState::Pressed && *shortcut == quick_shortcut {
				show_main(app, true);
			}
		})
		.build();

	tauri::Builder::default()
		.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
			show_main(app, args.iter().any(|arg| arg == QUICK_FLAG));
		}))
		.plugin(tauri_plugin_autostart::init(
			MacosLauncher::LaunchAgent,
			Some(vec![MINIMISED_FLAG]),
		))
		.plugin(shortcut_plugin)
		.plugin(tauri_plugin_dialog::init())
		.setup(move |app| {
			let state = commands::AppState::new(app.handle())?;
			app.manage(state);
			tray::create(app.handle())?;
			watch_tunnels(app.handle().clone());

			if let Err(error) = app.global_shortcut().register(quick_shortcut) {
				eprintln!(
					"hop: Ctrl+Shift+H is unavailable ({error}); bind a desktop shortcut to \
					 `hop {QUICK_FLAG}` instead"
				);
			}

			let args: Vec<String> = std::env::args().collect();
			if !args.iter().any(|arg| arg == MINIMISED_FLAG) {
				show_main(app.handle(), args.iter().any(|arg| arg == QUICK_FLAG));
			}

			// Off the setup thread so a slow config read or ssh start cannot hold up the window.
			let handle = app.handle().clone();
			thread::spawn(move || commands::start_auto_tunnels(&handle));

			Ok(())
		})
		.on_window_event(|window, event| {
			// Closing the window keeps Hop in the tray; Quit is in the tray menu.
			if let WindowEvent::CloseRequested { api, .. } = event {
				api.prevent_close();
				if let Err(error) = window.hide() {
					eprintln!("hop: could not hide the window: {error}");
				}
			}
		})
		.invoke_handler(tauri::generate_handler![
			commands::list_hosts,
			commands::host_detail,
			commands::connect,
			commands::save_host,
			commands::delete_host,
			commands::set_favourite,
			commands::check_status,
			commands::get_history,
			commands::clear_history,
			commands::get_settings,
			commands::save_settings,
			commands::list_tunnels,
			commands::start_tunnel,
			commands::stop_tunnel,
			commands::export_notes,
			commands::preview_notes_import,
			commands::apply_notes_import,
			commands::cancel_notes_import,
		])
		.build(tauri::generate_context!())
		.expect("error while building Hop")
		.run(|app, event| {
			// Tunnels are child processes; without this they would hold ports open after Quit.
			if let RunEvent::Exit = event {
				if let Some(state) = app.try_state::<commands::AppState>() {
					state.tunnels.stop_all();
				}
			}
		});
}

/// Checks once a second for tunnels whose ssh has died, so the window can say why.
fn watch_tunnels(app: AppHandle) {
	thread::spawn(move || loop {
		thread::sleep(Duration::from_secs(1));
		let state = app.state::<commands::AppState>();
		match state.tunnels.poll() {
			Ok(true) => commands::tunnels_changed(&app),
			Ok(false) => {}
			Err(error) => {
				eprintln!("hop: stopped watching tunnels: {error}");
				break;
			}
		}
	});
}

pub(crate) fn show_main(app: &AppHandle, quick: bool) {
	let Some(window) = app.get_webview_window("main")
	else {
		eprintln!("hop: main window is missing");
		return;
	};

	let shown = window
		.show()
		.and_then(|_| window.unminimize())
		.and_then(|_| window.set_focus());
	if let Err(error) = shown {
		eprintln!("hop: could not show the window: {error}");
	}

	if quick {
		if let Err(error) = app.emit(QUICK_LAUNCH_EVENT, ()) {
			eprintln!("hop: could not start quick launch: {error}");
		}
	}
}

/// Shows an error from the tray in the window, since the tray has nowhere to display it.
pub(crate) fn show_error(app: &AppHandle, message: &str) {
	show_main(app, false);
	notify_error(app, message);
}

/// Passes an error to the window without bringing it forward.
pub(crate) fn notify_error(app: &AppHandle, message: &str) {
	if let Err(error) = app.emit(ERROR_EVENT, message) {
		eprintln!("hop: could not show an error: {error}");
	}
}
