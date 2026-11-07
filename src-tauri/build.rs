// Only the commands listed here can be called from the page; each needs an `allow-*` entry in
// `capabilities/default.json` as well.
const COMMANDS: &[&str] = &[
	"list_hosts",
	"host_detail",
	"connect",
	"save_host",
	"delete_host",
	"set_favourite",
	"check_status",
	"get_history",
	"clear_history",
	"get_settings",
	"save_settings",
	"list_tunnels",
	"start_tunnel",
	"stop_tunnel",
];

fn main() {
	let manifest = tauri_build::AppManifest::new().commands(COMMANDS);
	tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
		.expect("failed to run tauri-build");
}
