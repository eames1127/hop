import { describe, expect, it } from "vitest";

import { buildSelection, exportableHosts, selectionCount } from "./exportSelection";
import type { Host, HostMeta } from "./types";

const noNotes: HostMeta = {
	description: null,
	tags: [],
	favourite: false,
	forwards: [],
	added: null,
};

function host(name: string, meta: Partial<HostMeta> = {}, editable = true): Host {
	return {
		id: name,
		name,
		kind: "host",
		editable,
		source: "/home/me/.ssh/config",
		included: false,
		hostName: null,
		user: null,
		port: null,
		proxyJump: null,
		identityFiles: [],
		missingKeys: [],
		meta: { ...noNotes, ...meta },
		lastUsed: null,
		useCount: 0,
		stale: false,
	};
}

const nas = host("nas", { tags: ["home", "storage"] });
const pi = host("pi", { tags: ["home"] });
const web = host("web", { description: "Work site" });
const bare = host("bare");
const wildcard = host("*", { tags: ["home"] }, false);
const all = [nas, pi, web, bare, wildcard];

describe("exportableHosts", () => {
	it("leaves out hosts without notes and read-only blocks", () => {
		expect(exportableHosts(all).map((item) => item.name)).toEqual(["nas", "pi", "web"]);
	});
});

describe("buildSelection", () => {
	it("keeps only the list that matches the mode", () => {
		expect(buildSelection("all", ["nas"], ["home"])).toEqual({ kind: "all" });
		expect(buildSelection("hosts", new Set(["nas"]), ["home"]))
			.toEqual({ kind: "hosts", names: ["nas"] });
		expect(buildSelection("tags", ["nas"], new Set(["home"])))
			.toEqual({ kind: "tags", tags: ["home"] });
	});
});

describe("selectionCount", () => {
	it("counts the hosts the export would write", () => {
		expect(selectionCount(all, { kind: "all" })).toBe(3);
		expect(selectionCount(all, { kind: "hosts", names: ["nas", "bare"] })).toBe(1);
		expect(selectionCount(all, { kind: "tags", tags: ["home"] })).toBe(2);
		expect(selectionCount(all, { kind: "tags", tags: [] })).toBe(0);
	});
});
