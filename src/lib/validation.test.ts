import { describe, expect, it } from "vitest";

import { dailyCounts, relativeTime } from "./time";
import {
	formatForwards,
	isValidHostName,
	parseForwards,
	parseTags,
	validateForm,
	type HostForm,
	withAutoStart,
} from "./validation";

const blank: HostForm = {
	name: "nas",
	hostName: "",
	user: "",
	port: "",
	identityFile: "",
	proxyJump: "",
	description: "",
	tags: "",
	favourite: false,
	forwards: "",
};

describe("host names", () => {
	it("refuses anything that could reach a shell", () => {
		expect(isValidHostName("web-01.example.com")).toBe(true);
		expect(isValidHostName("nas; rm -rf ~")).toBe(false);
		expect(isValidHostName("-oProxyCommand=calc")).toBe(false);
		expect(isValidHostName("")).toBe(false);
	});
});

describe("validateForm", () => {
	it("accepts a minimal host", () => {
		expect(validateForm(blank)).toEqual({});
	});

	it("explains each bad field", () => {
		const errors = validateForm({
			...blank,
			name: "bad name",
			hostName: "a b",
			port: "70000",
			identityFile: "\"quoted\"",
			forwards: "nope",
		});
		expect(Object.keys(errors).sort()).toEqual([
			"forwards",
			"hostName",
			"identityFile",
			"name",
			"port",
		]);
	});

	it("refuses line breaks that could add config options", () => {
		expect(validateForm({ ...blank, user: "me\nProxyCommand x" }).user).toBeDefined();
	});
});

describe("tags and forwards", () => {
	it("splits tags on commas", () => {
		expect(parseTags(" home, storage ,, ")).toEqual(["home", "storage"]);
	});

	it("round-trips forwards", () => {
		const text = "5001:localhost:5001 Admin UI\n8080:web:80";
		const forwards = parseForwards(text);
		expect(forwards).toHaveLength(2);
		expect(forwards?.[0].label).toBe("Admin UI");
		expect(formatForwards(forwards ?? [])).toBe(text);
		expect(parseForwards("0:localhost:22")).toBeNull();
	});

	it("puts the auto-start flag back by local port", () => {
		const forwards = parseForwards("5001:localhost:5001 Admin\n8080:web:80") ?? [];
		expect(forwards.every((forward) => !forward.autoStart)).toBe(true);
		const flagged = withAutoStart(forwards, [8080, 9999]);
		expect(flagged.map((forward) => forward.autoStart)).toEqual([false, true]);
	});
});

describe("time", () => {
	it("describes recent times", () => {
		expect(relativeTime(null)).toBe("Never");
		expect(relativeTime(1000, 1030)).toBe("Just now");
		expect(relativeTime(1000, 1000 + 3 * 86400)).toBe("3 days ago");
	});

	it("buckets connections by day", () => {
		const now = new Date(2026, 10, 6, 12);
		const today = now.getTime() / 1000;
		const buckets = dailyCounts([today, today - 3600, today - 86400 * 2], 7, now);
		expect(buckets).toHaveLength(7);
		expect(buckets[6].count).toBe(2);
		expect(buckets[4].count).toBe(1);
	});
});
