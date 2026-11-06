import { describe, expect, it } from "vitest";

import { filterHosts, needsAttention, sortHosts, tagCounts } from "./filter";
import type { Host } from "./types";

function host(name: string, overrides: Partial<Host> = {}): Host {
	return {
		id: name,
		name,
		kind: "host",
		editable: true,
		source: "/home/me/.ssh/config",
		included: false,
		hostName: `${name}.lan`,
		user: null,
		port: null,
		proxyJump: null,
		identityFiles: [],
		missingKeys: [],
		meta: { description: null, tags: [], favourite: false, forwards: [], added: null },
		lastUsed: null,
		useCount: 0,
		stale: false,
		...overrides,
	};
}

const nas = host("nas", {
	meta: {
		description: "Synology NAS",
		tags: ["home", "storage"],
		favourite: true,
		forwards: [],
		added: null,
	},
});
const pi = host("pi", { stale: true, lastUsed: 100 });
const web = host("web", { lastUsed: 500, meta: { ...nas.meta, tags: ["work"], favourite: false } });
const wildcard = host("*", { editable: false });
const all = [pi, web, nas, wildcard];

describe("filterHosts", () => {
	it("matches every word against name, address and notes", () => {
		const result = filterHosts(all, { query: "synology", tag: null, section: "all" });
		expect(result.map((item) => item.name)).toEqual(["web", "nas"]);
		const narrowed = filterHosts(all, { query: "nas home", tag: null, section: "all" });
		expect(narrowed.map((item) => item.name)).toEqual(["nas"]);
	});

	it("filters by tag and section", () => {
		expect(filterHosts(all, { query: "", tag: "work", section: "all" })).toEqual([web]);
		expect(filterHosts(all, { query: "", tag: null, section: "favourites" })).toEqual([nas]);
	});

	it("lists stale, broken and offline hosts as needing attention", () => {
		const broken = host("old", { missingKeys: ["~/.ssh/gone"] });
		const offline = new Set(["web"]);
		const result = filterHosts([...all, broken], {
			query: "",
			tag: null,
			section: "attention",
		}, offline);
		expect(result.map((item) => item.name)).toEqual(["pi", "web", "old"]);
		expect(needsAttention(wildcard, new Set(["*"]))).toBe(false);
	});
});

describe("sortHosts", () => {
	it("puts favourites, then recent, then read-only blocks", () => {
		expect(sortHosts(all).map((item) => item.name)).toEqual(["nas", "web", "pi", "*"]);
	});
});

describe("tagCounts", () => {
	it("counts tags alphabetically", () => {
		expect(tagCounts(all)).toEqual([["home", 1], ["storage", 1], ["work", 1]]);
	});
});
