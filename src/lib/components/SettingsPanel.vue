<script setup lang="ts">
import { onMounted, ref } from "vue";

import {
	applyNotesImport,
	cancelNotesImport,
	clearHistory,
	errorMessage,
	exportNotes,
	getSettings,
	previewNotesImport,
	saveSettings,
} from "../api";
import type { ListMode, NotesImportPreview, Settings, SettingsView } from "../types";

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

onMounted(async () => {
	try {
		view.value = await getSettings();
		draft.value = { ...view.value.settings };
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

function exportAll(): Promise<void> {
	return runNotes(async () => {
		const count = await exportNotes();
		if (count !== null) {
			notesMessage.value = `Exported notes for ${plural(count, "host")}.`;
		}
	});
}

function chooseImport(): Promise<void> {
	return runNotes(async () => {
		importPreview.value = await previewNotesImport();
	});
}

function confirmImport(): Promise<void> {
	const preview = importPreview.value;
	if (preview === null) {
		return Promise.resolve();
	}

	return runNotes(async () => {
		await applyNotesImport(listMode.value);
		importPreview.value = null;
		notesMessage.value = `Imported notes for ${plural(preview.matched.length, "host")}.`;
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

				<section class="detail__section">
					<h3>History</h3>
					<p class="muted small">
						Hop records only the host name and time of each connection.
					</p>
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
						Copy descriptions, tags, favourites and port forwards to another computer.
						Addresses, users and key files are not included, but host aliases are, and
						they may still reveal host names.
					</p>
					<div class="notes-io__buttons">
						<button
							type="button"
							class="button"
							:disabled="busy"
							@click="exportAll"
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
					config file it changes, and never adds hosts from an import.
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
				<p v-else>
					None of the hosts in this file are in your SSH config.
				</p>

				<template v-if="importPreview.unknown.length > 0">
					<h3>Skipping {{ plural(importPreview.unknown.length, "unknown host") }}</h3>
					<p class="muted small">
						These are not editable hosts in your SSH config. Add them first, then import
						again.
					</p>
					<ul class="notes-io__list">
						<li
							v-for="alias in importPreview.unknown"
							:key="alias"
						>
							{{ alias }}
						</li>
					</ul>
				</template>

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
						:disabled="busy || importPreview.matched.length === 0"
						@click="confirmImport"
					>
						Import
					</button>
				</div>
			</div>
		</div>
	</div>
</template>
