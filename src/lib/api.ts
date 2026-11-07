// Typed wrappers around the Rust commands. These are the only calls the page can make.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
	HistoryEntry,
	HostDetail,
	HostInput,
	HostList,
	HostStatus,
	Settings,
	SettingsView,
	TunnelView,
} from "./types";

export function listHosts(): Promise<HostList> {
	return invoke("list_hosts");
}

export function hostDetail(name: string): Promise<HostDetail> {
	return invoke("host_detail", { name });
}

export function connect(name: string, forward: number | null = null): Promise<number> {
	return invoke("connect", { name, forward });
}

export function saveHost(original: string | null, input: HostInput): Promise<HostList> {
	return invoke("save_host", { request: { original, input } });
}

export function deleteHost(name: string): Promise<HostList> {
	return invoke("delete_host", { name });
}

export function setFavourite(name: string, favourite: boolean): Promise<HostList> {
	return invoke("set_favourite", { name, favourite });
}

export function checkStatus(names: string[]): Promise<HostStatus[]> {
	return invoke("check_status", { names });
}

export function getHistory(name: string | null, days: number): Promise<HistoryEntry[]> {
	return invoke("get_history", { name, days });
}

export function clearHistory(): Promise<void> {
	return invoke("clear_history");
}

export function getSettings(): Promise<SettingsView> {
	return invoke("get_settings");
}

export function saveSettings(settings: Settings): Promise<SettingsView> {
	return invoke("save_settings", { settings });
}

export function listTunnels(): Promise<TunnelView[]> {
	return invoke("list_tunnels");
}

export function startTunnel(name: string, forward: number): Promise<TunnelView[]> {
	return invoke("start_tunnel", { name, forward });
}

export function stopTunnel(localPort: number): Promise<TunnelView[]> {
	return invoke("stop_tunnel", { localPort });
}

export function onTunnelsChanged(handler: () => void): Promise<UnlistenFn> {
	return listen("tunnels-changed", handler);
}

export function onHostsChanged(handler: () => void): Promise<UnlistenFn> {
	return listen("hosts-changed", handler);
}

export function onQuickLaunch(handler: () => void): Promise<UnlistenFn> {
	return listen("quick-launch", handler);
}

export function onError(handler: (message: string) => void): Promise<UnlistenFn> {
	return listen<string>("hop-error", (event) => handler(event.payload));
}

// Commands reject with a plain string; anything else is shown as-is.
export function errorMessage(error: unknown): string {
	if (typeof error === "string") {
		return error;
	}
	else if (error instanceof Error) {
		return error.message;
	}

	return String(error);
}
