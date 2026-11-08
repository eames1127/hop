// Mirrors the checks in crates/sshconf so the form can explain problems before saving.
// The Rust side checks again; this is only for friendlier messages.

import type { Forward } from "./types";

export const HOST_NAME_PATTERN = /^[A-Za-z0-9._-]+$/;

export interface HostForm {
	name: string;
	hostName: string;
	user: string;
	port: string;
	identityFile: string;
	proxyJump: string;
	description: string;
	tags: string;
	favourite: boolean;
	forwards: string;
}

export type FormErrors = Partial<Record<keyof HostForm, string>>;

export function isValidHostName(name: string): boolean {
	return HOST_NAME_PATTERN.test(name) && !name.startsWith("-") && name.length <= 253;
}

function isPlainValue(value: string): boolean {
	return !/[\s"#]/.test(value) && !hasControlCharacter(value);
}

function hasControlCharacter(value: string): boolean {
	return [...value].some((character) => {
		const code = character.charCodeAt(0);
		return code < 32 || code === 127;
	});
}

export function parseTags(text: string): string[] {
	return text
		.split(",")
		.map((tag) => tag.trim())
		.filter((tag) => tag.length > 0);
}

// One forward per line: `local:host:remote Label`.
export function parseForwards(text: string): Forward[] | null {
	const forwards: Forward[] = [];
	const lines = text
		.split("\n")
		.map((line) => line.trim())
		.filter((line) => line.length > 0);
	for (const line of lines) {
		const match = /^(\d{1,5}):([A-Za-z0-9._-]+):(\d{1,5})(?:\s+(.*))?$/.exec(line);
		if (match === null) {
			return null;
		}

		const localPort = Number(match[1]);
		const remotePort = Number(match[3]);
		if (!isPort(localPort) || !isPort(remotePort) || match[2].startsWith("-")) {
			return null;
		}

		forwards.push({
			localPort,
			remoteHost: match[2],
			remotePort,
			label: (match[4] ?? "").trim(),
			autoStart: false,
		});
	}

	return forwards;
}

function isPort(value: number): boolean {
	return Number.isInteger(value) && value >= 1 && value <= 65535;
}

// The text box has no room for the flag, so it is kept per local port and put back on save;
// a local port can only be forwarded once, which makes it a stable key while the text changes.
export function withAutoStart(forwards: Forward[], ports: number[]): Forward[] {
	return forwards.map((forward) => ({
		...forward,
		autoStart: ports.includes(forward.localPort),
	}));
}

export function formatForwards(forwards: Forward[]): string {
	return forwards
		.map((forward) => {
			const spec = `${forward.localPort}:${forward.remoteHost}:${forward.remotePort}`;
			return forward.label ? `${spec} ${forward.label}` : spec;
		})
		.join("\n");
}

export function validateForm(form: HostForm): FormErrors {
	const errors: FormErrors = {};
	if (!isValidHostName(form.name.trim())) {
		errors.name = "Use letters, numbers, dots, dashes and underscores only.";
	}

	if (form.hostName.trim() !== "" && !isPlainValue(form.hostName.trim())) {
		errors.hostName = "The address cannot contain spaces.";
	}

	if (form.user.trim() !== "" && !isPlainValue(form.user.trim())) {
		errors.user = "The user name cannot contain spaces.";
	}

	if (form.port.trim() !== "" && !isPort(Number(form.port.trim()))) {
		errors.port = "Enter a port from 1 to 65535.";
	}

	if (/["\r\n]/.test(form.identityFile)) {
		errors.identityFile = "The key path cannot contain quotes.";
	}

	if (form.proxyJump.trim() !== "" && !isPlainValue(form.proxyJump.trim())) {
		errors.proxyJump = "The jump host cannot contain spaces.";
	}

	if (parseForwards(form.forwards) === null) {
		errors.forwards = "Write each forward as local:host:remote, "
			+ "for example 5001:localhost:5001.";
	}

	return errors;
}
