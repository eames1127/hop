<script setup lang="ts">
import { computed, reactive, ref } from "vue";

import { errorMessage, saveHost } from "../api";
import type { Host, HostInput, HostList } from "../types";
import {
	formatForwards,
	parseForwards,
	parseTags,
	validateForm,
	type HostForm,
} from "../validation";

const props = defineProps<{
	host: Host | null;
}>();

const emit = defineEmits<{
	saved: [list: HostList];
	cancel: [];
}>();

const form = reactive<HostForm>({
	name: props.host?.name ?? "",
	hostName: props.host?.hostName ?? "",
	user: props.host?.user ?? "",
	port: props.host?.port ?? "",
	identityFile: props.host?.identityFiles[0] ?? "",
	proxyJump: props.host?.proxyJump ?? "",
	description: props.host?.meta.description ?? "",
	tags: props.host?.meta.tags.join(", ") ?? "",
	favourite: props.host?.meta.favourite ?? false,
	forwards: formatForwards(props.host?.meta.forwards ?? []),
});

const submitted = ref(false);
const saving = ref(false);
const saveError = ref<string | null>(null);
const errors = computed(() => validateForm(form));
const shownErrors = computed(() => (submitted.value ? errors.value : {}));
const title = computed(() => (props.host ? `Edit ${props.host.name}` : "Add host"));

function optional(value: string): string | null {
	const trimmed = value.trim();
	return trimmed === "" ? null : trimmed;
}

function toInput(): HostInput {
	return {
		name: form.name.trim(),
		hostName: form.hostName.trim(),
		user: optional(form.user),
		port: optional(form.port) === null ? null : Number(form.port.trim()),
		identityFile: optional(form.identityFile),
		proxyJump: optional(form.proxyJump),
		meta: {
			description: optional(form.description),
			tags: parseTags(form.tags),
			favourite: form.favourite,
			forwards: parseForwards(form.forwards) ?? [],
			added: props.host?.meta.added ?? null,
		},
	};
}

async function submit(): Promise<void> {
	submitted.value = true;
	saveError.value = null;
	if (Object.keys(errors.value).length > 0) {
		return;
	}

	saving.value = true;
	try {
		const list = await saveHost(props.host?.name ?? null, toInput());
		emit("saved", list);
	}
	catch (error) {
		saveError.value = errorMessage(error);
	}
	finally {
		saving.value = false;
	}
}
</script>

<template>
	<div
		class="overlay"
		@click.self="emit('cancel')"
	>
		<form
			class="dialog"
			novalidate
			@submit.prevent="submit"
			@keydown.esc="emit('cancel')"
		>
			<h2>{{ title }}</h2>
			<p class="muted small">
				Hop backs up your config before saving and only rewrites this host's block.
			</p>

			<div class="form-grid">
				<label class="field">
					<span>Name</span>
					<input
						v-model="form.name"
						autofocus
						placeholder="nas"
						spellcheck="false"
					>
					<small
						v-if="shownErrors.name"
						class="error-text"
					>{{ shownErrors.name }}</small>
				</label>
				<label class="field">
					<span>Address</span>
					<input
						v-model="form.hostName"
						placeholder="192.168.1.20 or nas.lan"
						spellcheck="false"
					>
					<small
						v-if="shownErrors.hostName"
						class="error-text"
					>{{ shownErrors.hostName }}</small>
				</label>
				<label class="field">
					<span>User</span>
					<input
						v-model="form.user"
						spellcheck="false"
					>
					<small
						v-if="shownErrors.user"
						class="error-text"
					>{{ shownErrors.user }}</small>
				</label>
				<label class="field">
					<span>Port</span>
					<input
						v-model="form.port"
						inputmode="numeric"
						placeholder="22"
					>
					<small
						v-if="shownErrors.port"
						class="error-text"
					>{{ shownErrors.port }}</small>
				</label>
				<label class="field">
					<span>Key file</span>
					<input
						v-model="form.identityFile"
						placeholder="~/.ssh/id_ed25519"
						spellcheck="false"
					>
					<small
						v-if="shownErrors.identityFile"
						class="error-text"
					>{{ shownErrors.identityFile }}</small>
				</label>
				<label class="field">
					<span>Jump host</span>
					<input
						v-model="form.proxyJump"
						placeholder="bastion"
						spellcheck="false"
					>
					<small
						v-if="shownErrors.proxyJump"
						class="error-text"
					>{{ shownErrors.proxyJump }}</small>
				</label>
				<label class="field field--wide">
					<span>Description</span>
					<input v-model="form.description">
				</label>
				<label class="field field--wide">
					<span>Tags, separated by commas</span>
					<input
						v-model="form.tags"
						placeholder="home, storage"
					>
				</label>
				<label class="field field--wide">
					<span>Port forwards, one per line</span>
					<textarea
						v-model="form.forwards"
						rows="3"
						placeholder="5001:localhost:5001 Admin UI"
						spellcheck="false"
					/>
					<small
						v-if="shownErrors.forwards"
						class="error-text"
					>{{ shownErrors.forwards }}</small>
				</label>
				<label class="checkbox field--wide">
					<input
						v-model="form.favourite"
						type="checkbox"
					>
					<span>Favourite (shown first and in the tray menu)</span>
				</label>
			</div>

			<p
				v-if="saveError"
				class="error-text"
				role="alert"
			>
				{{ saveError }}
			</p>

			<div class="dialog__actions">
				<button
					type="button"
					class="button"
					@click="emit('cancel')"
				>
					Cancel
				</button>
				<button
					type="submit"
					class="button button--primary"
					:disabled="saving"
				>
					{{ saving ? "Saving…" : "Save" }}
				</button>
			</div>
		</form>
	</div>
</template>
