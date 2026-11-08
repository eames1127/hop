export type GridKey = "ArrowLeft" | "ArrowRight" | "ArrowUp" | "ArrowDown" | "Home" | "End";

const gridKeys: readonly string[] = [
	"ArrowLeft",
	"ArrowRight",
	"ArrowUp",
	"ArrowDown",
	"Home",
	"End",
];

export function isGridKey(key: string): key is GridKey {
	return gridKeys.includes(key);
}

// Moves within a grid laid out row by row. Edges clamp rather than wrap, so holding an arrow
// key stops at the last card instead of jumping back to the start.
export function moveInGrid(index: number, count: number, columns: number, key: GridKey): number {
	if (count <= 0) {
		return -1;
	}

	const width = Math.max(1, columns);
	const last = count - 1;
	if (index < 0 || index > last) {
		return 0;
	}

	let next: number;
	if (key === "ArrowLeft") {
		next = index - 1;
	}
	else if (key === "ArrowRight") {
		next = index + 1;
	}
	else if (key === "ArrowUp") {
		next = index - width;
	}
	else if (key === "ArrowDown") {
		// A short last row should still be reachable from the row above it.
		next = index + width > last && Math.floor(index / width) < Math.floor(last / width)
			? last
			: index + width;
	}
	else if (key === "Home") {
		next = 0;
	}
	else {
		next = last;
	}

	if (next < 0 || next > last) {
		return index;
	}

	return next;
}

// Counts the tracks the browser resolved for a CSS grid, e.g. "260px 260px 260px" is three.
export function columnCount(gridTemplateColumns: string): number {
	const tracks = gridTemplateColumns
		.trim()
		.split(/\s+/)
		.filter((track) => track.length > 0 && track !== "none");

	return Math.max(1, tracks.length);
}

interface KeyTarget {
	tagName?: string;
	isContentEditable?: boolean;
}

// Shortcuts must not steal keys from text the user is typing.
export function isTypingTarget(target: KeyTarget | null): boolean {
	if (!target) {
		return false;
	}
	else if (target.isContentEditable) {
		return true;
	}

	const tag = (target.tagName ?? "").toUpperCase();
	return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
}

export interface ShortcutEvent {
	key: string;
	ctrlKey: boolean;
	metaKey: boolean;
	altKey: boolean;
}

export function isSearchShortcut(event: ShortcutEvent, typing: boolean): boolean {
	if (event.altKey) {
		return false;
	}
	else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "f") {
		return true;
	}

	return !typing && !event.ctrlKey && !event.metaKey && event.key === "/";
}
