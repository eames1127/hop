import { describe, expect, it } from "vitest";

import { findTunnel, forwardName, tunnelAction, tunnelTitle } from "./tunnels";
import type { Forward, TunnelView } from "./types";

const forward: Forward = { localPort: 8080, remoteHost: "localhost", remotePort: 80, label: "" };

function tunnel(overrides: Partial<TunnelView> = {}): TunnelView {
	return {
		host: "nas",
		forward: 0,
		localPort: 8080,
		remoteHost: "localhost",
		remotePort: 80,
		label: "",
		state: "running",
		error: null,
		...overrides,
	};
}

describe("findTunnel", () => {
	it("matches on host and forward position", () => {
		const tunnels = [tunnel({ host: "web" }), tunnel({ forward: 1, localPort: 9090 })];
		expect(findTunnel(tunnels, "nas", 1)?.localPort).toBe(9090);
		expect(findTunnel(tunnels, "nas", 0)).toBeNull();
	});
});

describe("tunnelAction", () => {
	it("stops running tunnels and starts the rest", () => {
		expect(tunnelAction(tunnel())).toBe("stop");
		expect(tunnelAction(tunnel({ state: "failed" }))).toBe("start");
		expect(tunnelAction(null)).toBe("start");
	});
});

describe("labels", () => {
	it("falls back to the local port", () => {
		expect(forwardName(forward)).toBe("Forward 8080");
		expect(forwardName({ ...forward, label: "Admin" })).toBe("Admin");
	});

	it("explains the state in the title", () => {
		expect(tunnelTitle(forward, null)).toContain("Open a background tunnel");
		expect(tunnelTitle(forward, tunnel())).toContain("Click to stop");
		const failed = tunnel({ state: "failed", error: "Port busy." });
		expect(tunnelTitle(forward, failed)).toContain("Port busy.");
	});
});
