// Mirrors the serialised Rust types in src-tauri/src and crates/sshconf.

export interface Forward {
	localPort: number;
	remoteHost: string;
	remotePort: number;
	label: string;
	// Opened as a background tunnel when Hop starts.
	autoStart: boolean;
}

export interface HostMeta {
	description: string | null;
	tags: string[];
	favourite: boolean;
	forwards: Forward[];
	added: string | null;
}

export type BlockKind = "host" | "match";

export interface Host {
	id: string;
	name: string;
	kind: BlockKind;
	editable: boolean;
	source: string;
	included: boolean;
	hostName: string | null;
	user: string | null;
	port: string | null;
	proxyJump: string | null;
	identityFiles: string[];
	missingKeys: string[];
	meta: HostMeta;
	lastUsed: number | null;
	useCount: number;
	stale: boolean;
}

export interface HostList {
	hosts: Host[];
	warnings: string[];
	configPath: string;
}

export interface HostInput {
	name: string;
	hostName: string;
	user: string | null;
	port: number | null;
	identityFile: string | null;
	proxyJump: string | null;
	meta: HostMeta;
}

export interface ResolvedHost {
	hostName: string;
	user: string | null;
	port: number;
	proxyJump: string | null;
	identityFiles: string[];
}

export interface HistoryEntry {
	host: string;
	startedAt: number;
	durationSecs: number | null;
}

export interface HostDetail {
	resolved: ResolvedHost | null;
	error: string | null;
	history: HistoryEntry[];
}

export type Reachability = "online" | "offline" | "unknown";

export interface HostStatus {
	name: string;
	state: Reachability;
	detail: string | null;
	latencyMs: number | null;
}

export type Theme = "system" | "light" | "dark";

export interface Settings {
	terminal: string;
	theme: Theme;
	startWithComputer: boolean;
	staleDays: number;
	// 0 keeps history forever.
	historyRetentionDays: number;
	autoStatusChecks: boolean;
}

export interface TerminalChoice {
	id: string;
	label: string;
}

export interface SettingsView {
	settings: Settings;
	terminals: TerminalChoice[];
	configPath: string;
	dataPath: string;
}

export type TunnelState = "running" | "failed";

export interface TunnelView {
	host: string;
	forward: number;
	localPort: number;
	remoteHost: string;
	remotePort: number;
	label: string;
	state: TunnelState;
	error: string | null;
}

// How an import treats tags and port forwards the host already has.
export type ListMode = "overwrite" | "merge";

export type ExportSelection =
	| { kind: "all" }
	| { kind: "hosts"; names: string[] }
	| { kind: "tags"; tags: string[] };

export interface NotesImportPreview {
	fileName: string;
	matched: string[];
	unknown: string[];
}

export interface KeyInfo {
	// As ssh-keygen names it, such as "ED25519" or "RSA".
	kind: string;
	bits: number;
	fingerprint: string;
	comment: string;
	publicKey: string;
}

export interface KeyFile {
	file: string;
	info: KeyInfo | null;
	error: string | null;
}

export type AuthoriseOutcome = "added" | "alreadyPresent" | "failed";

export interface AuthoriseResult {
	name: string;
	outcome: AuthoriseOutcome;
	detail: string | null;
}
