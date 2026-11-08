import type { Host } from "./types";

export type Section = "all" | "favourites" | "attention";

export interface HostFilter {
	query: string;
	// Selected tags combine with AND: each click narrows the list, as search words do.
	tags: readonly string[];
	section: Section;
}

// A host needs attention if it is stale, has a missing key, or failed its online check.
export function needsAttention(host: Host, offline: ReadonlySet<string>): boolean {
	if (!host.editable) {
		return false;
	}

	return host.stale || host.missingKeys.length > 0 || offline.has(host.name);
}

function matchesQuery(host: Host, query: string): boolean {
	const words = query
		.toLowerCase()
		.split(/\s+/)
		.filter((word) => word.length > 0);
	if (words.length === 0) {
		return true;
	}

	const haystack = [
		host.name,
		host.hostName ?? "",
		host.user ?? "",
		host.meta.description ?? "",
		...host.meta.tags,
	]
		.join(" ")
		.toLowerCase();

	return words.every((word) => haystack.includes(word));
}

export function filterHosts(
	hosts: Host[],
	filter: HostFilter,
	offline: ReadonlySet<string> = new Set(),
): Host[] {
	return hosts.filter((host) => {
		if (filter.section === "favourites" && !host.meta.favourite) {
			return false;
		}
		else if (filter.section === "attention" && !needsAttention(host, offline)) {
			return false;
		}

		if (!filter.tags.every((tag) => host.meta.tags.includes(tag))) {
			return false;
		}

		return matchesQuery(host, filter.query);
	});
}

// Favourites first, then most recently used, then by name. Read-only blocks go last.
export function sortHosts(hosts: Host[]): Host[] {
	return [...hosts].sort((a, b) => {
		if (a.editable !== b.editable) {
			return a.editable ? -1 : 1;
		}
		else if (a.meta.favourite !== b.meta.favourite) {
			return a.meta.favourite ? -1 : 1;
		}
		else if ((a.lastUsed ?? 0) !== (b.lastUsed ?? 0)) {
			return (b.lastUsed ?? 0) - (a.lastUsed ?? 0);
		}

		return a.name.localeCompare(b.name);
	});
}

export function tagCounts(hosts: Host[]): Array<[string, number]> {
	const counts = new Map<string, number>();
	for (const host of hosts) {
		for (const tag of host.meta.tags) {
			counts.set(tag, (counts.get(tag) ?? 0) + 1);
		}
	}

	return [...counts.entries()].sort((a, b) => a[0].localeCompare(b[0]));
}

// Clicking a tag adds it to the filter, clicking it again removes it.
export function toggleTag(selected: readonly string[], tag: string): string[] {
	if (selected.includes(tag)) {
		return selected.filter((item) => item !== tag);
	}

	return [...selected, tag];
}
