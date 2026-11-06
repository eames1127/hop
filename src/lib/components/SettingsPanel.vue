<script setup lang="ts">
import { onMounted, ref } from "vue";

import { clearHistory, errorMessage, getSettings, saveSettings } from "../api";
import type { Settings, SettingsView } from "../types";

const emit = defineEmits<{
	close: [];
	saved: [settings: Settings];
}>();

const view = ref<SettingsView | null>(null);
const draft = ref<Settings | null>(null);
const message = ref<string | null>(null);
const failure = ref<string | null>(null);
const busy = ref(false);

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
	</div>
</template>
