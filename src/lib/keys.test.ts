import { describe, expect, it } from "vitest";

import { authorisableHosts, shellQuote, suggestKeyName, targetNames } from "./keys";
import type { Host } from "./types";

function host(name: string, tags: string[] = [], editable = true): Host {
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
		meta: { description: null, tags, favourite: false, forwards: [], added: null },
		lastUsed: null,
		useCount: 0,
		stale: false,
	};
}

describe("authorisableHosts", () => {
	it("leaves out read-only blocks, which Hop cannot sign in to by name", () => {
		const hosts = [host("nas"), host("*", [], false)];
		expect(authorisableHosts(hosts).map((h) => h.name)).toEqual(["nas"]);
	});
});

describe("targetNames", () => {
	const hosts = [host("nas", ["home"]), host("pi", ["home", "lab"]), host("web", ["work"])];

	it("keeps chosen hosts in list order", () => {
		expect(targetNames(hosts, "hosts", ["web", "nas"], [])).toEqual(["nas", "web"]);
	});

	it("takes every host with any chosen tag, once", () => {
		expect(targetNames(hosts, "tags", [], ["home", "lab"])).toEqual(["nas", "pi"]);
	});

	it("ignores names that are not authorisable", () => {
		const withWildcard = [...hosts, host("*", ["home"], false)];
		expect(targetNames(withWildcard, "hosts", ["*"], [])).toEqual([]);
		expect(targetNames(withWildcard, "tags", [], ["home"])).toEqual(["nas", "pi"]);
	});
});

describe("shellQuote", () => {
	it("keeps text literal for a POSIX shell", () => {
		expect(shellQuote("ssh-ed25519 AAAA dan@pc")).toBe("'ssh-ed25519 AAAA dan@pc'");
		expect(shellQuote("Ann's $(laptop)")).toBe("'Ann'\\''s $(laptop)'");
	});
});

describe("suggestKeyName", () => {
	it("uses the OpenSSH default when it is free", () => {
		expect(suggestKeyName([])).toBe("id_ed25519");
		expect(suggestKeyName(["id_rsa.pub"])).toBe("id_ed25519");
	});

	it("numbers the name when the default is taken", () => {
		expect(suggestKeyName(["id_ed25519.pub"])).toBe("id_ed25519_2");
		expect(suggestKeyName(["id_ed25519.pub", "id_ed25519_2.pub"])).toBe("id_ed25519_3");
	});
});
