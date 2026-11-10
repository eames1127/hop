<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";

import { authoriseKey, createKey, errorMessage, inspectPublicKey, listKeys } from "../api";
import { tagCounts } from "../filter";
import {
	authorisableHosts,
	shellQuote,
	suggestKeyName,
	targetNames,
	type TargetMode,
} from "../keys";
import type { AuthoriseResult, Host, KeyFile, KeyInfo } from "../types";

const props = defineProps<{
	hosts: Host[];
}>();

const keys = ref<KeyFile[]>([]);
const failure = ref<string | null>(null);
const message = ref<string | null>(null);
const busy = ref(false);
const creating = ref(false);
const newKeyName = ref("");

const authorising = ref(false);
const pasted = ref("");
const inspected = ref<KeyInfo | null>(null);
const targetMode = ref<TargetMode>("hosts");
const targetHosts = ref<string[]>([]);
const targetTags = ref<string[]>([]);
const results = ref<AuthoriseResult[] | null>(null);

const hostChoices = computed(() => authorisableHosts(props.hosts));
const tagChoices = computed(() => tagCounts(hostChoices.value).map(([tag]) => tag));
const targets = computed(() => targetNames(
	props.hosts,
	targetMode.value,
	targetHosts.value,
	targetTags.value,
));
const manualKey = computed(() => inspected.value === null
	? "'ssh-ed25519 AAAA… name@computer'"
	: shellQuote(inspected.value.publicKey));

// A changed key must be checked again before it can be sent anywhere.
watch(pasted, () => {
	inspected.value = null;
	results.value = null;
});

onMounted(refresh);

async function run(action: () => Promise<void>): Promise<void> {
	busy.value = true;
	failure.value = null;
	message.value = null;
	try {
		await action();
	}
	catch (error) {
		failure.value = errorMessage(error);
	}
	finally {
		busy.value = false;
	}
}

function refresh(): Promise<void> {
	return run(async () => {
		keys.value = await listKeys();
	});
}

async function copy(info: KeyInfo): Promise<void> {
	failure.value = null;
	try {
		await navigator.clipboard.writeText(info.publicKey);
		message.value = "Public key copied.";
	}
	catch (error) {
		failure.value = `Could not copy: ${errorMessage(error)}`;
	}
}

function startCreating(): void {
	newKeyName.value = suggestKeyName(keys.value.map((key) => key.file));
	creating.value = true;
}

function create(): Promise<void> {
	return run(async () => {
		await createKey(newKeyName.value.trim());
		creating.value = false;
		message.value = "Finish in the terminal, then choose Refresh to see the new key.";
	});
}

function openAuthorise(): void {
	pasted.value = "";
	results.value = null;
	failure.value = null;
	authorising.value = true;
}

function closeAuthorise(): void {
	authorising.value = false;
}

function inspect(): Promise<void> {
	return run(async () => {
		inspected.value = await inspectPublicKey(pasted.value);
	});
}

function authorise(): Promise<void> {
	if (inspected.value === null) {
		return Promise.resolve();
	}

	const publicKey = inspected.value.publicKey;
	return run(async () => {
		results.value = await authoriseKey(targets.value, publicKey);
	});
}

function outcomeText(result: AuthoriseResult): string {
	if (result.outcome === "added") {
		return "Added";
	}
	else if (result.outcome === "alreadyPresent") {
		return "Already there";
	}

	return result.detail ?? "Failed";
}

function resultClass(result: AuthoriseResult): string[] {
	return ["authorise-results__item", `authorise-results__item--${result.outcome}`];
}

function plural(count: number, word: string): string {
	return `${count} ${word}${count === 1 ? "" : "s"}`;
}
</script>

<template>
	<section class="detail__section">
		<h3>SSH keys</h3>
		<p class="muted small">
			Public keys in your SSH folder. Hop only reads <code>.pub</code> files and never opens
			or moves a private key.
		</p>

		<p
			v-if="failure && !authorising"
			class="error-text"
			role="alert"
		>
			{{ failure }}
		</p>

		<ul
			v-if="keys.length > 0"
			class="key-list"
		>
			<li
				v-for="key in keys"
				:key="key.file"
				class="key-list__item"
			>
				<div class="key-list__text">
					<code>{{ key.file }}</code>
					<template v-if="key.info">
						<span class="muted small">
							{{ key.info.kind }} {{ key.info.bits }}
							<template v-if="key.info.comment">· {{ key.info.comment }}</template>
						</span>
						<code class="muted small">{{ key.info.fingerprint }}</code>
					</template>
					<span
						v-else
						class="error-text small"
					>{{ key.error }}</span>
				</div>
				<button
					v-if="key.info"
					type="button"
					class="button"
					@click="copy(key.info)"
				>
					Copy public key
				</button>
			</li>
		</ul>
		<p
			v-else
			class="muted small"
		>
			No keys in your SSH folder yet.
		</p>

		<div
			v-if="creating"
			class="key-create"
		>
			<label class="field">
				<span>Name for the new key</span>
				<input
					v-model="newKeyName"
					maxlength="64"
					spellcheck="false"
					@keydown.enter.prevent="create"
				>
			</label>
			<p class="muted small">
				Opens a terminal running <code>ssh-keygen</code>, which asks for an optional
				passphrase there. Hop never sees it.
			</p>
			<div class="notes-io__buttons">
				<button
					type="button"
					class="button button--primary"
					:disabled="busy || newKeyName.trim() === ''"
					@click="create"
				>
					Create
				</button>
				<button
					type="button"
					class="button"
					@click="creating = false"
				>
					Cancel
				</button>
			</div>
		</div>

		<div class="notes-io__buttons">
			<button
				v-if="!creating"
				type="button"
				class="button"
				:disabled="busy"
				@click="startCreating"
			>
				New key…
			</button>
			<button
				type="button"
				class="button"
				:disabled="busy"
				@click="openAuthorise"
			>
				Authorise a key on hosts…
			</button>
			<button
				type="button"
				class="button"
				:disabled="busy"
				@click="refresh"
			>
				Refresh
			</button>
		</div>
		<p
			v-if="message"
			class="muted small"
		>
			{{ message }}
		</p>

		<details class="help">
			<summary>Setting up another computer</summary>
			<ol>
				<li>
					Copy your SSH config (<code>~/.ssh/config</code>) to the other computer. Hosts
					whose key is not there are flagged as missing a key.
				</li>
				<li>
					On the other computer, choose <strong>New key…</strong>, then
					<strong>Copy public key</strong>.
				</li>
				<li>
					Send the public key here. Email or chat is fine: a public key is not secret,
					and the fingerprint check below catches one that was changed on the way.
				</li>
				<li>
					Here, choose <strong>Authorise a key on hosts…</strong>, paste it and check
					the fingerprint matches the one the other computer shows.
				</li>
				<li>
					If your hosts name a key file, give the new key that same name, or edit the
					hosts to use it. The missing-key flags clear once the names match.
				</li>
			</ol>
			<p class="muted small">
				Each computer keeps its own key. If one is lost, remove its line from
				<code>authorized_keys</code> and the other keeps working.
			</p>
		</details>

		<div
			v-if="authorising"
			class="overlay"
			@click.self="closeAuthorise"
		>
			<div
				class="dialog"
				role="dialog"
				aria-labelledby="authorise-title"
				@keydown.esc.stop="closeAuthorise"
			>
				<h2 id="authorise-title">
					Authorise a key on hosts
				</h2>
				<p class="muted small">
					Hop signs in with this computer's access and adds the key to each host's
					<code>~/.ssh/authorized_keys</code>. Hosts that need a password are skipped.
				</p>

				<p
					v-if="failure"
					class="error-text"
					role="alert"
				>
					{{ failure }}
				</p>

				<label class="field">
					<span>Public key (the <code>.pub</code> file's contents)</span>
					<textarea
						v-model="pasted"
						rows="3"
						spellcheck="false"
						placeholder="ssh-ed25519 AAAA… name@computer"
					/>
				</label>

				<button
					v-if="inspected === null"
					type="button"
					class="button"
					:disabled="busy || pasted.trim() === ''"
					@click="inspect"
				>
					Check key
				</button>

				<template v-else>
					<div class="key-check">
						<span class="muted small">
							{{ inspected.kind }} {{ inspected.bits }}
							<template v-if="inspected.comment">· {{ inspected.comment }}</template>
						</span>
						<code>{{ inspected.fingerprint }}</code>
						<span class="muted small">
							Check this matches the fingerprint shown on the other computer.
						</span>
					</div>

					<fieldset class="notes-io__mode">
						<legend>Add it to</legend>
						<label class="checkbox">
							<input
								v-model="targetMode"
								type="radio"
								value="hosts"
							>
							<span>Chosen hosts</span>
						</label>
						<div
							v-if="targetMode === 'hosts'"
							class="notes-io__choices"
						>
							<label
								v-for="host in hostChoices"
								:key="host.id"
								class="checkbox"
							>
								<input
									v-model="targetHosts"
									type="checkbox"
									:value="host.name"
								>
								<span>{{ host.name }}</span>
							</label>
						</div>
						<label class="checkbox">
							<input
								v-model="targetMode"
								type="radio"
								value="tags"
							>
							<span>Hosts with chosen tags</span>
						</label>
						<div
							v-if="targetMode === 'tags'"
							class="notes-io__choices"
						>
							<label
								v-for="tag in tagChoices"
								:key="tag"
								class="checkbox"
							>
								<input
									v-model="targetTags"
									type="checkbox"
									:value="tag"
								>
								<span>{{ tag }}</span>
							</label>
						</div>
						<p class="muted small">
							Will sign in to {{ plural(targets.length, "host") }}.
						</p>
					</fieldset>
				</template>

				<ul
					v-if="results"
					class="authorise-results"
				>
					<li
						v-for="result in results"
						:key="result.name"
						:class="resultClass(result)"
					>
						<strong>{{ result.name }}</strong>
						<span>{{ outcomeText(result) }}</span>
					</li>
				</ul>

				<details class="help">
					<summary>What happens on the server, and doing it by hand</summary>
					<p class="small">
						The key is added as one line to <code>~/.ssh/authorized_keys</code> for the
						user Hop signs in as. <code>~/.ssh</code> and the file are created private
						if they are missing; existing lines are never changed. To do the same on
						the server yourself:
					</p>
					<pre class="help__code"><code>mkdir -p ~/.ssh &amp;&amp; chmod 700 ~/.ssh
echo {{ manualKey }} &gt;&gt; ~/.ssh/authorized_keys
chmod 600 ~/.ssh/authorized_keys</code></pre>
					<p class="small">
						If the other computer still cannot sign in:
					</p>
					<ul class="small">
						<li>
							<code>PubkeyAuthentication</code> must not be <code>no</code> in
							<code>/etc/ssh/sshd_config</code>.
						</li>
						<li>
							Your home folder and <code>~/.ssh</code> must not be writable by other
							users, or sshd ignores the file.
						</li>
						<li>
							On SELinux systems (Fedora, RHEL), run
							<code>restorecon -R ~/.ssh</code>.
						</li>
						<li>
							Windows servers keep administrators' keys in
							<code>C:\ProgramData\ssh\administrators_authorized_keys</code>; Hop
							cannot add keys there.
						</li>
					</ul>
					<p class="small">
						Once both computers sign in with keys, you can set
						<code>PasswordAuthentication no</code> on the server. To revoke a computer
						later, delete its line from <code>authorized_keys</code>.
					</p>
				</details>

				<div class="dialog__actions">
					<button
						type="button"
						class="button"
						@click="closeAuthorise"
					>
						{{ results ? "Done" : "Cancel" }}
					</button>
					<button
						type="button"
						class="button button--primary"
						:disabled="busy || inspected === null || targets.length === 0"
						@click="authorise"
					>
						{{ busy && inspected ? "Authorising…" : "Authorise" }}
					</button>
				</div>
			</div>
		</div>
	</section>
</template>
