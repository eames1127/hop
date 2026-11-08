import { describe, expect, it } from "vitest";

import { columnCount, isGridKey, isSearchShortcut, isTypingTarget, moveInGrid } from "./keyboard";

const plain = { ctrlKey: false, metaKey: false, altKey: false };

describe("moveInGrid", () => {
	// Seven cards in three columns: rows [0 1 2] [3 4 5] [6].
	it("moves sideways and between rows", () => {
		expect(moveInGrid(1, 7, 3, "ArrowRight")).toBe(2);
		expect(moveInGrid(1, 7, 3, "ArrowLeft")).toBe(0);
		expect(moveInGrid(1, 7, 3, "ArrowDown")).toBe(4);
		expect(moveInGrid(4, 7, 3, "ArrowUp")).toBe(1);
	});

	it("stays put at the edges", () => {
		expect(moveInGrid(0, 7, 3, "ArrowLeft")).toBe(0);
		expect(moveInGrid(1, 7, 3, "ArrowUp")).toBe(1);
		expect(moveInGrid(6, 7, 3, "ArrowRight")).toBe(6);
		expect(moveInGrid(6, 7, 3, "ArrowDown")).toBe(6);
	});

	it("drops into a short last row", () => {
		expect(moveInGrid(5, 7, 3, "ArrowDown")).toBe(6);
	});

	it("jumps to the ends and copes with empty or unfocused grids", () => {
		expect(moveInGrid(3, 7, 3, "Home")).toBe(0);
		expect(moveInGrid(3, 7, 3, "End")).toBe(6);
		expect(moveInGrid(-1, 7, 3, "ArrowDown")).toBe(0);
		expect(moveInGrid(0, 0, 3, "ArrowDown")).toBe(-1);
	});
});

describe("columnCount", () => {
	it("counts resolved grid tracks", () => {
		expect(columnCount("260px 260px 260px")).toBe(3);
		expect(columnCount("none")).toBe(1);
		expect(columnCount("")).toBe(1);
	});
});

describe("isGridKey", () => {
	it("accepts arrows only", () => {
		expect(isGridKey("ArrowUp")).toBe(true);
		expect(isGridKey("a")).toBe(false);
	});
});

describe("isTypingTarget", () => {
	it("spots text fields", () => {
		expect(isTypingTarget({ tagName: "INPUT" })).toBe(true);
		expect(isTypingTarget({ tagName: "textarea" })).toBe(true);
		expect(isTypingTarget({ tagName: "DIV", isContentEditable: true })).toBe(true);
		expect(isTypingTarget({ tagName: "ARTICLE" })).toBe(false);
		expect(isTypingTarget(null)).toBe(false);
	});
});

describe("isSearchShortcut", () => {
	it("treats Ctrl+F as search everywhere and / only outside text fields", () => {
		expect(isSearchShortcut({ ...plain, key: "f", ctrlKey: true }, true)).toBe(true);
		expect(isSearchShortcut({ ...plain, key: "/" }, false)).toBe(true);
		expect(isSearchShortcut({ ...plain, key: "/" }, true)).toBe(false);
		expect(isSearchShortcut({ ...plain, key: "f" }, false)).toBe(false);
		expect(isSearchShortcut({ ...plain, key: "f", ctrlKey: true, altKey: true }, false))
			.toBe(false);
	});
});
