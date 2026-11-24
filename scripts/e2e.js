// End-to-end smoke test for the Windows debug build. It launches the built app against a copy of
// e2e/fixtures/ssh, drives the WebView2 page over the Chrome DevTools Protocol and checks that
// cards render, search filters, an invalid host name is refused, and adding, editing, starring and
// deleting a host each rewrite only that host's block, with a backup. It uses only Node's built-in
// fetch and WebSocket (Node 22+), so it adds no dependencies.
//
// Build first with `pnpm tauri build --debug --no-bundle`, then run `pnpm test:e2e`.

import { spawn, spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdtempSync, readdirSync, readFileSync, rmSync } from "node:fs";
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

// Fills a form field found by its visible label, since the inputs carry no names or ids.
function fillField(label, text) {
	const wanted = JSON.stringify(label);
	return `(() => {
		const field = [...document.querySelectorAll(".dialog label.field")]
			.find((candidate) => candidate.querySelector("span")?.textContent.trim() === ${wanted});
		const input = field.querySelector("input, textarea");
		input.value = ${JSON.stringify(text)};
		input.dispatchEvent(new Event("input", { bubbles: true }));
		return true;
	})()`;
}

function clickButton(scope, text) {
	return `[...document.querySelectorAll(${JSON.stringify(`${scope} button`)})]
		.find((button) => button.textContent.trim() === ${JSON.stringify(text)}).click()`;
}

function cardSelector(name) {
	const wanted = JSON.stringify(name);
	return `[...document.querySelectorAll(".host-card")]
		.find((card) => card.querySelector(".host-card__name").textContent.trim() === ${wanted})`;
}

const countCards = "document.querySelectorAll('.host-card').length";

// Splits a config into its `Host` blocks, keyed by alias, so a check can say exactly which
// block a save changed and that every other block kept its bytes.
function hostBlocks(text) {
	const blocks = new Map();
	let current = null;
	for (const line of text.split(/(?<=\n)/)) {
		const match = /^Host\s+(\S+)/.exec(line);
		if (match) {
			current = match[1];
			blocks.set(current, "");
		}
		if (current !== null) {
			blocks.set(current, blocks.get(current) + line);
		}
	}
	return blocks;
}

function readConfig(sshDir) {
	return readFileSync(join(sshDir, "config"), "utf8");
}

function backups(sshDir) {
	return readdirSync(sshDir)
		.filter((name) => name.startsWith("config.hop-backup-"))
		.sort();
}

// Fails unless the only blocks that differ between the two configs are the named ones.
function expectOnlyChanged(before, after, changed, action) {
	const old = hostBlocks(before);
	const fresh = hostBlocks(after);
	for (const name of new Set([...old.keys(), ...fresh.keys()])) {
		if (changed.includes(name)) {
			continue;
		}
		// A new block is appended after a blank line, which the splitter files under the block
		// before it, so trailing blank lines are separators rather than part of a block.
		const trim = (block) => block?.replace(/(\r?\n)+$/, "");
		if (trim(old.get(name)) !== trim(fresh.get(name))) {
			throw new Error(`${action} changed the ${name} block as well`);
		}
	}
}

async function waitForConfig(sshDir, description, check) {
	return waitFor(description, () => {
		const text = readConfig(sshDir);
		return check(text) ? text : null;
	});
}

async function checkConfigWrites(page, sshDir) {
	const original = readConfig(sshDir);

	await page.evaluate(clickButton(".toolbar", "Add host"));
	await waitFor("the add host form", () => page.evaluate("!!document.querySelector('.dialog')"));
	await page.evaluate(fillField("Name", "delta-new"));
	await page.evaluate(fillField("Address", "192.0.2.40"));
	await page.evaluate(fillField("User", "tester"));
	await page.evaluate(fillField("Description", "Added by the smoke test"));
	await page.evaluate("document.querySelector('.dialog button[type=submit]').click()");
	await waitFor("a fourth card", async () => (await page.evaluate(countCards)) === 4);
	const added = await waitForConfig(sshDir, "the new block", (text) => {
		return hostBlocks(text).get("delta-new")
			?.includes("192.0.2.40");
	});
	expectOnlyChanged(original, added, ["delta-new"], "Adding a host");
	const firstBackups = backups(sshDir);
	if (firstBackups.length !== 1) {
		throw new Error(`Expected one backup after adding a host, found ${firstBackups.length}`);
	}
	if (readFileSync(join(sshDir, firstBackups[0]), "utf8") !== original) {
		throw new Error("The backup does not hold the config as it was before the save");
	}
	console.log("ok - adding a host writes only its block and backs up the config first");

	await page.evaluate(`${cardSelector("beta-db")}.click()`);
	await waitFor("the beta-db details", () => page.evaluate(
		"document.querySelector('.detail h2')?.textContent.trim() === 'beta-db'",
	));
	await page.evaluate(clickButton(".detail", "Edit"));
	await waitFor("the edit form", () => page.evaluate("!!document.querySelector('.dialog')"));
	await page.evaluate(fillField("Address", "192.0.2.21"));
	await page.evaluate(fillField("Description", "Database, edited by the smoke test"));
	await page.evaluate("document.querySelector('.dialog button[type=submit]').click()");
	const edited = await waitForConfig(sshDir, "the edited block", (text) => {
		return hostBlocks(text).get("beta-db")
			?.includes("192.0.2.21");
	});
	expectOnlyChanged(added, edited, ["beta-db"], "Editing beta-db");
	if (backups(sshDir).length !== 2) {
		throw new Error(`Expected two backups after an edit, found ${backups(sshDir).length}`);
	}
	console.log("ok - editing a host changes only its block");

	await page.evaluate(`${cardSelector("alpha-web")}.querySelector(".icon-button").click()`);
	const starred = await waitForConfig(sshDir, "the favourite note", (text) => {
		return hostBlocks(text).get("alpha-web")
			?.includes("# fav: true");
	});
	expectOnlyChanged(edited, starred, ["alpha-web"], "Starring alpha-web");
	console.log("ok - starring a host writes a favourite note to its block only");

	// window.confirm is the page's own dialog, so the test can answer it without a native window.
	await page.evaluate(`${cardSelector("gamma-cache")}.click()`);
	await waitFor("the gamma-cache details", () => page.evaluate(
		"document.querySelector('.detail h2')?.textContent.trim() === 'gamma-cache'",
	));
	await page.evaluate("window.confirm = () => false");
	await page.evaluate(clickButton(".detail", "Delete"));
	await sleep(1000);
	if (readConfig(sshDir) !== starred) {
		throw new Error("The config changed after a delete was cancelled");
	}
	console.log("ok - a cancelled delete leaves the config alone");

	await page.evaluate("window.confirm = () => true");
	await page.evaluate(clickButton(".detail", "Delete"));
	await waitFor("three cards again", async () => (await page.evaluate(countCards)) === 3);
	const deleted = await waitForConfig(sshDir, "the block to go", (text) => {
		return !hostBlocks(text).has("gamma-cache");
	});
	expectOnlyChanged(starred, deleted, ["gamma-cache"], "Deleting gamma-cache");
	console.log("ok - deleting a host removes only its block");
}

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
	await page.evaluate(clickButton(".dialog", "Cancel"));
	await waitFor("the form to close", () => page.evaluate("!document.querySelector('.dialog')"));

	await checkConfigWrites(page, sshDir);
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
			HOP_E2E_DEBUG_PORT: String(port),
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
