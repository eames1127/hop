<script setup lang="ts">
import { computed, onMounted, ref } from "vue";

import {
	applyNotesImport,
	cancelNotesImport,
	clearHistory,
	errorMessage,
	exportNotes,
	getSettings,
	listHosts,
	previewNotesImport,
	saveSettings,
} from "../api";
import {
	buildSelection,
	exportableHosts,
	selectionCount,
	type ExportMode,
} from "../exportSelection";
import { tagCounts } from "../filter";
import type { Host, ListMode, NotesImportPreview, Settings, SettingsView } from "../types";
import KeysSection from "./KeysSection.vue";

const emit = defineEmits<{
	close: [];
	saved: [settings: Settings];
}>();

const view = ref<SettingsView | null>(null);
const draft = ref<Settings | null>(null);
const message = ref<string | null>(null);
const failure = ref<string | null>(null);
const busy = ref(false);
const notesMessage = ref<string | null>(null);
const importPreview = ref<NotesImportPreview | null>(null);
const listMode = ref<ListMode>("merge");
const addNew = ref(true);
const includeConnection = ref(true);
const hosts = ref<Host[]>([]);
const exportMode = ref<ExportMode>("all");
const exportNames = ref<string[]>([]);
const exportTags = ref<string[]>([]);

const exportChoices = computed(() => exportableHosts(hosts.value, includeConnection.value));
const exportTagChoices = computed(() => tagCounts(exportChoices.value).map(([tag]) => tag));
const selection = computed(() => buildSelection(
	exportMode.value,
	exportNames.value,
	exportTags.value,
));
const exportCount = computed(() => selectionCount(
	hosts.value,
	selection.value,
	includeConnection.value,
));
const importAdds = computed(() => (addNew.value ? importPreview.value?.new.length ?? 0 : 0));
const missingKeyCount = computed(() => importPreview.value?.new
	.filter((host) => host.keyMissing).length ?? 0);

onMounted(async () => {
	try {
		const [settings, list] = await Promise.all([getSettings(), listHosts()]);
		view.value = settings;
		draft.value = { ...settings.settings };
		hosts.value = list.hosts;
	}
	catch (error) {
		failure.value = errorMessage(error);
	}
});

async function save(): Promise<void> {
	if (draft.value === null) {
		return;
	}

	busy.value = true;
	failure.value = null;
	try {
		view.value = await saveSettings({ ...draft.value });
		emit("saved", view.value.settings);
		emit("close");
	}
	catch (error) {
		failure.value = errorMessage(error);
	}
	finally {
		busy.value = false;
	}
}

async function clear(): Promise<void> {
	if (!window.confirm("Clear all connection history? This cannot be undone.")) {
		return;
	}

	failure.value = null;
	try {
		await clearHistory();
		message.value = "History cleared.";
	}
	catch (error) {
		failure.value = errorMessage(error);
	}
}

function plural(count: number, word: string): string {
	return `${count} ${word}${count === 1 ? "" : "s"}`;
}

async function runNotes(action: () => Promise<void>): Promise<void> {
	busy.value = true;
	failure.value = null;
	notesMessage.value = null;
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

function exportChosen(): Promise<void> {
	return runNotes(async () => {
		const count = await exportNotes(selection.value, includeConnection.value);
		if (count !== null) {
			notesMessage.value = `Exported ${plural(count, "host")}.`;
		}
	});
}

function chooseImport(): Promise<void> {
	return runNotes(async () => {
		addNew.value = true;
		importPreview.value = await previewNotesImport();
	});
}

function confirmImport(): Promise<void> {
	const preview = importPreview.value;
	if (preview === null) {
		return Promise.resolve();
	}

	const added = importAdds.value;
	return runNotes(async () => {
		hosts.value = (await applyNotesImport(listMode.value, addNew.value)).hosts;
		importPreview.value = null;
		const parts = [`updated ${plural(preview.matched.length, "host")}`];
		if (added > 0) {
			parts.push(`added ${plural(added, "new host")}`);
		}

		notesMessage.value = `Import done: ${parts.join(" and ")}.`;
	});
}

async function closeImport(): Promise<void> {
	importPreview.value = null;
	try {
		await cancelNotesImport();
	}
	catch (error) {
		failure.value = errorMessage(error);
	}
}
</script>

<template>
	<div
		class="overlay"
		@click.self="emit('close')"
	>
		<form
			class="dialog"
			@submit.prevent="save"
			@keydown.esc="emit('close')"
		>
			<h2>Settings</h2>

			<p
				v-if="failure"
				class="error-text"
				role="alert"
			>
				{{ failure }}
			</p>

			<template v-if="draft && view">
				<label class="field">
					<span>Open connections in</span>
					<select v-model="draft.terminal">
						<option
							v-for="terminal in view.terminals"
							:key="terminal.id"
							:value="terminal.id"
						>
							{{ terminal.label }}
						</option>
					</select>
				</label>

				<label class="field">
					<span>Theme</span>
					<select v-model="draft.theme">
						<option value="system">Match the system</option>
						<option value="light">Light</option>
						<option value="dark">Dark</option>
					</select>
				</label>

				<label class="field">
					<span>Flag a host as stale after this many unused days</span>
					<input
						v-model.number="draft.staleDays"
						type="number"
						min="1"
						max="3650"
					>
				</label>

				<label class="checkbox">
					<input
						v-model="draft.startWithComputer"
						type="checkbox"
					>
					<span>Start Hop in the tray when the computer starts</span>
				</label>

				<label class="checkbox">
					<input
						v-model="draft.autoStatusChecks"
						type="checkbox"
					>
					<span>Check whether hosts are online when Hop opens</span>
				</label>

				<section class="detail__section">
					<h3>History</h3>
					<p class="muted small">
						Hop records only the host name and time of each connection.
					</p>
					<label class="field">
						<span>Keep history for</span>
						<select v-model.number="draft.historyRetentionDays">
							<option :value="30">30 days</option>
							<option :value="90">90 days</option>
							<option :value="365">365 days</option>
							<option :value="0">Forever</option>
						</select>
					</label>
					<button
						type="button"
						class="button button--danger"
						@click="clear"
					>
						Clear history
					</button>
					<span
						v-if="message"
						class="muted"
					> {{ message }}</span>
				</section>

				<section class="detail__section">
					<h3>Notes</h3>
					<p class="muted small">
						Copy hosts with their descriptions, tags, favourites and port forwards to
						another computer. Key file paths can be included, but keys themselves never
						are: each computer keeps its own.
					</p>
					<label class="checkbox">
						<input
							v-model="includeConnection"
							type="checkbox"
						>
						<span>Include addresses, users, ports, jump hosts and key file paths</span>
					</label>
					<p
						v-if="!includeConnection"
						class="muted small"
					>
						Host aliases are still included, and may reveal host names.
					</p>
					<fieldset class="notes-io__mode">
						<legend>Export</legend>
						<label class="checkbox">
							<input
								v-model="exportMode"
								type="radio"
								value="all"
							>
							<span>All hosts</span>
						</label>
						<label class="checkbox">
							<input
								v-model="exportMode"
								type="radio"
								value="hosts"
							>
							<span>Chosen hosts</span>
						</label>
						<div
							v-if="exportMode === 'hosts'"
							class="notes-io__choices"
						>
							<label
								v-for="host in exportChoices"
								:key="host.id"
								class="checkbox"
							>
								<input
									v-model="exportNames"
									type="checkbox"
									:value="host.name"
								>
								<span>{{ host.name }}</span>
							</label>
						</div>
						<label class="checkbox">
							<input
								v-model="exportMode"
								type="radio"
								value="tags"
							>
							<span>Hosts with chosen tags</span>
						</label>
						<div
							v-if="exportMode === 'tags'"
							class="notes-io__choices"
						>
							<label
								v-for="tag in exportTagChoices"
								:key="tag"
								class="checkbox"
							>
								<input
									v-model="exportTags"
									type="checkbox"
									:value="tag"
								>
								<span>{{ tag }}</span>
							</label>
						</div>
						<p class="muted small">
							Will export {{ plural(exportCount, "host") }}.
						</p>
					</fieldset>
					<div class="notes-io__buttons">
						<button
							type="button"
							class="button"
							:disabled="busy || exportCount === 0"
							@click="exportChosen"
						>
							Export…
						</button>
						<button
							type="button"
							class="button"
							:disabled="busy"
							@click="chooseImport"
						>
							Import…
						</button>
					</div>
					<p
						v-if="notesMessage"
						class="muted small"
					>
						{{ notesMessage }}
					</p>
				</section>

				<KeysSection :hosts="hosts" />

				<section class="detail__section">
					<h3>Files</h3>
					<p class="muted small">
						SSH config: <code>{{ view.configPath }}</code>
					</p>
					<p class="muted small">
						Hop's data: <code>{{ view.dataPath }}</code>
					</p>
					<p class="muted small">
						Quick launch: Ctrl+Shift+H, or bind a desktop shortcut to
						<code>hop --quick</code> (recommended on KDE Plasma under Wayland).
					</p>
				</section>
			</template>

			<div class="dialog__actions">
				<button
					type="button"
					class="button"
					@click="emit('close')"
				>
					Cancel
				</button>
				<button
					type="submit"
					class="button button--primary"
					:disabled="busy || draft === null"
				>
					Save
				</button>
			</div>
		</form>

		<div
			v-if="importPreview"
			class="overlay"
			@click.self="closeImport"
		>
			<div
				class="dialog"
				role="dialog"
				aria-labelledby="import-title"
				@keydown.esc.stop="closeImport"
			>
				<h2 id="import-title">
					Import notes
				</h2>
				<p class="muted small">
					From <code>{{ importPreview.fileName }}</code>. Hop keeps a backup of each
					config file it changes.
				</p>

				<template v-if="importPreview.matched.length > 0">
					<h3>Will update {{ plural(importPreview.matched.length, "host") }}</h3>
					<ul class="notes-io__list">
						<li
							v-for="alias in importPreview.matched"
							:key="alias"
						>
							{{ alias }}
						</li>
					</ul>

					<fieldset class="notes-io__mode">
						<legend>Tags and port forwards already on these hosts</legend>
						<label class="checkbox">
							<input
								v-model="listMode"
								type="radio"
								value="merge"
							>
							<span>Keep them and add the imported ones</span>
						</label>
						<label class="checkbox">
							<input
								v-model="listMode"
								type="radio"
								value="overwrite"
							>
							<span>Replace them with the imported ones</span>
						</label>
					</fieldset>
				</template>

				<template v-if="importPreview.new.length > 0">
					<h3>{{ plural(importPreview.new.length, "host") }} not in your SSH config</h3>
					<label class="checkbox">
						<input
							v-model="addNew"
							type="checkbox"
						>
						<span>Add them to your SSH config</span>
					</label>
					<ul class="notes-io__list">
						<li
							v-for="host in importPreview.new"
							:key="host.name"
						>
							{{ host.name }}
							<span
								v-if="host.hostName === null"
								class="muted small"
							> (no address, edit it after importing)</span>
							<span
								v-if="host.keyMissing"
								class="error-text small"
							> (key <code>{{ host.identityFile }}</code> not on this computer)</span>
						</li>
					</ul>
					<p
						v-if="missingKeyCount > 0"
						class="muted small"
					>
						Keys are not copied between computers. Create a key for this computer under
						SSH keys in settings, then add it to those servers from a computer that can
						already sign in.
					</p>
				</template>

				<template v-if="importPreview.conflicting.length > 0">
					<h3>Skipping {{ plural(importPreview.conflicting.length, "host") }}</h3>
					<p class="muted small">
						These names are already used by blocks Hop does not edit, such as a
						<code>Host</code> line with several names.
					</p>
					<ul class="notes-io__list">
						<li
							v-for="alias in importPreview.conflicting"
							:key="alias"
						>
							{{ alias }}
						</li>
					</ul>
				</template>

				<p v-if="importPreview.matched.length === 0 && importPreview.new.length === 0">
					None of the hosts in this file can be imported.
				</p>

				<div class="dialog__actions">
					<button
						type="button"
						class="button"
						@click="closeImport"
					>
						Cancel
					</button>
					<button
						type="button"
						class="button button--primary"
						:disabled="busy || (importPreview.matched.length === 0 && importAdds === 0)"
						@click="confirmImport"
					>
						Import
					</button>
				</div>
			</div>
		</div>
	</div>
</template>
