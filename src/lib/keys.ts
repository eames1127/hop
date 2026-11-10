import type { Host } from "./types";

export type TargetMode = "hosts" | "tags";

const DEFAULT_KEY_NAME = "id_ed25519";

// Mirrors the Rust check, which only signs in to hosts the config names in an editable block.
export function authorisableHosts(hosts: Host[]): Host[] {
	return hosts.filter((host) => host.editable);
}

export function targetNames(
	hosts: Host[],
	mode: TargetMode,
	names: string[],
	tags: string[],
): string[] {
	return authorisableHosts(hosts)
		.filter((host) => mode === "hosts"
			? names.includes(host.name)
			: host.meta.tags.some((tag) => tags.includes(tag)))
		.map((host) => host.name);
}

// For the by-hand commands shown to the user: a key's comment may contain a quote.
export function shellQuote(text: string): string {
	return `'${text.replaceAll("'", "'\\''")}'`;
}

// The listing only shows .pub files, so a private key without one could still be in the way;
// the Rust side refuses that name rather than overwrite it.
export function suggestKeyName(publicKeyFiles: string[]): string {
	const taken = new Set(publicKeyFiles.map((file) => file.replace(/\.pub$/, "")));
	if (!taken.has(DEFAULT_KEY_NAME)) {
		return DEFAULT_KEY_NAME;
	}

	let number = 2;
	while (taken.has(`${DEFAULT_KEY_NAME}_${number}`)) {
		number += 1;
	}

	return `${DEFAULT_KEY_NAME}_${number}`;
}
