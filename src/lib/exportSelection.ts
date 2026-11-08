import type { ExportSelection, Host, HostMeta } from "./types";

export type ExportMode = ExportSelection["kind"];

// Mirrors the Rust export, which leaves out read-only blocks and hosts without notes.
function hasNotes(meta: HostMeta): boolean {
	return meta.description !== null
		|| meta.tags.length > 0
		|| meta.favourite
		|| meta.forwards.length > 0
		|| meta.added !== null;
}

export function exportableHosts(hosts: Host[]): Host[] {
	return hosts.filter((host) => host.editable && hasNotes(host.meta));
}

export function buildSelection(
	mode: ExportMode,
	names: Iterable<string>,
	tags: Iterable<string>,
): ExportSelection {
	if (mode === "hosts") {
		return { kind: "hosts", names: [...names] };
	}
	else if (mode === "tags") {
		return { kind: "tags", tags: [...tags] };
	}

	return { kind: "all" };
}

function isSelected(host: Host, selection: ExportSelection): boolean {
	if (selection.kind === "hosts") {
		return selection.names.includes(host.name);
	}
	else if (selection.kind === "tags") {
		return host.meta.tags.some((tag) => selection.tags.includes(tag));
	}

	return true;
}

// How many hosts the export would write, shown before the save dialog opens.
export function selectionCount(hosts: Host[], selection: ExportSelection): number {
	return exportableHosts(hosts).filter((host) => isSelected(host, selection)).length;
}
