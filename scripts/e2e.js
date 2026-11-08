// End-to-end smoke test for the Windows debug build. It launches the built app against a copy of
// e2e/fixtures/ssh, drives the WebView2 page over the Chrome DevTools Protocol and checks that
// cards render, search filters and an invalid host name is refused. It uses only Node's built-in
// fetch and WebSocket (Node 22+), so it adds no dependencies.
//
// Build first with `pnpm tauri build --debug --no-bundle`, then run `pnpm test:e2e`.

import { spawn, spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const port = Number(process.env.HOP_E2E_PORT ?? 9333);
const binary = process.env.HOP_E2E_BINARY ?? join(root, "target", "debug", "hop.exe");
const timeoutMs = 60_000;

function sleep(ms) {
	return new Promise((done) => setTimeout(done, ms));
}

async function waitFor(description, check) {
	const deadline = Date.now() + timeoutMs;
	let lastError;
	while (Date.now() < deadline) {
		try {
			const value = await check();
			if (value) {
				return value;
			}
		}
		catch (error) {
			lastError = error;
		}
		await sleep(250);
	}
	const reason = lastError ? ` (last error: ${lastError.message})` : "";
	throw new Error(`Timed out waiting for ${description}${reason}`);
}

async function findPage() {
	const response = await fetch(`http://127.0.0.1:${port}/json/list`);
	const targets = await response.json();
	return targets.find((target) => target.type === "page" && target.webSocketDebuggerUrl);
}

function connect(url) {
	const socket = new WebSocket(url);
	const pending = new Map();
	let nextId = 1;
	socket.addEventListener("message", (event) => {
		const message = JSON.parse(event.data);
		const request = pending.get(message.id);
		if (!request) {
			return;
		}
		pending.delete(message.id);
		if (message.error) {
			request.reject(new Error(message.error.message));
		}
		else {
			request.resolve(message.result);
		}
	});
	const opened = new Promise((done, fail) => {
		socket.addEventListener("open", done, { once: true });
		socket.addEventListener("error", () => fail(new Error("WebSocket failed")), { once: true });
	});
	return opened.then(() => ({
		close: () => socket.close(),
		async evaluate(expression) {
			const id = nextId++;
			const result = new Promise((done, fail) => {
				pending.set(id, { resolve: done, reject: fail });
			});
			socket.send(JSON.stringify({
				id,
				method: "Runtime.evaluate",
				params: { expression, awaitPromise: true, returnByValue: true },
			}));
			const { result: value, exceptionDetails } = await result;
			if (exceptionDetails) {
				throw new Error(exceptionDetails.exception?.description ?? exceptionDetails.text);
			}
			return value.value;
		},
	}));
}

// Vue's v-model listens for input events, so setting .value alone is not enough.
function typeInto(selector, text) {
	return `(() => {
		const input = document.querySelector(${JSON.stringify(selector)});
		input.value = ${JSON.stringify(text)};
		input.dispatchEvent(new Event("input", { bubbles: true }));
		return true;
	})()`;
}

const countCards = "document.querySelectorAll('.host-card').length";

async function run(page, sshDir) {
	await waitFor("three host cards", async () => {
		return (await page.evaluate(countCards)) === 3;
	});
	console.log("ok - fixture hosts render as cards");

	await page.evaluate(typeInto(".search", "beta"));
	await waitFor("search to leave one card", async () => {
		return (await page.evaluate(countCards)) === 1;
	});
	const name = await page.evaluate("document.querySelector('.host-card__name').textContent");
	if (!name.includes("beta-db")) {
		throw new Error(`Search kept the wrong card: ${name.trim()}`);
	}
	await page.evaluate(typeInto(".search", ""));
	await waitFor("search to clear", async () => (await page.evaluate(countCards)) === 3);
	console.log("ok - search filters the cards");

	const configBefore = readFileSync(join(sshDir, "config"), "utf8");
	await page.evaluate(`[...document.querySelectorAll(".toolbar button")]
		.find((button) => button.textContent.trim() === "Add host").click()`);
	await waitFor("the add host form", () => page.evaluate("!!document.querySelector('.dialog')"));
	await page.evaluate(typeInto(".dialog input", "bad name"));
	await page.evaluate("document.querySelector('.dialog button[type=submit]').click()");
	const error = await waitFor("a host name error", () => page.evaluate(
		"document.querySelector('.dialog .error-text')?.textContent.trim()",
	));
	if (readFileSync(join(sshDir, "config"), "utf8") !== configBefore) {
		throw new Error("The config changed after an invalid host name was submitted");
	}
	console.log(`ok - an invalid host name is refused ("${error}")`);
}

function stop(app) {
	if (app.exitCode !== null) {
		return;
	}
	if (process.platform === "win32") {
		spawnSync("taskkill", ["/pid", String(app.pid), "/T", "/F"], { stdio: "ignore" });
	}
	else {
		app.kill();
	}
}

// A failed run in CI leaves nothing to inspect, so print whether Hop and WebView2 were running
// and what was listening before the processes are stopped.
function describeProcesses() {
	if (process.platform !== "win32") {
		return;
	}
	for (const [command, args] of [
		["tasklist", ["/fi", "imagename eq hop.exe"]],
		["tasklist", ["/fi", "imagename eq msedgewebview2.exe"]],
		["netstat", ["-ano", "-p", "tcp"]],
	]) {
		const output = spawnSync(command, args, { encoding: "utf8" }).stdout ?? "";
		console.error(`--- ${command} ${args.join(" ")}\n${output.trim()}`);
	}
}

async function main() {
	if (!existsSync(binary)) {
		throw new Error(`${binary} not found; run \`pnpm tauri build --debug --no-bundle\` first`);
	}
	// Work on a copy so a failing run can never alter the committed fixture.
	const sshDir = mkdtempSync(join(tmpdir(), "hop-e2e-"));
	cpSync(join(root, "e2e", "fixtures", "ssh"), sshDir, { recursive: true });

	const app = spawn(binary, [], {
		stdio: "inherit",
		env: {
			...process.env,
			HOP_SSH_DIR: sshDir,
			WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
		},
	});
	let stopping = false;
	app.on("exit", (code) => {
		if (!stopping) {
			console.error(`Hop exited early with code ${code}`);
		}
	});

	let page;
	try {
		const target = await waitFor("the WebView2 debugging port", () => {
			if (app.exitCode !== null) {
				throw new Error(`Hop is no longer running (exit code ${app.exitCode})`);
			}
			return findPage();
		});
		page = await connect(target.webSocketDebuggerUrl);
		await run(page, sshDir);
		console.log("All end-to-end checks passed.");
	}
	catch (error) {
		describeProcesses();
		throw error;
	}
	finally {
		page?.close();
		stopping = true;
		stop(app);
		rmSync(sshDir, { recursive: true, force: true });
	}
}

main().catch((error) => {
	console.error(`not ok - ${error.message}`);
	process.exitCode = 1;
});
