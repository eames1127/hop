<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref } from "vue";

import {
	checkStatus,
	connect,
	deleteHost,
	errorMessage,
	getSettings,
	listHosts,
	onError,
	onHostsChanged,
	onQuickLaunch,
	setFavourite,
} from "../lib/api";
import AddHostForm from "../lib/components/AddHostForm.vue";
import HostCard from "../lib/components/HostCard.vue";
import HostDetail from "../lib/components/HostDetail.vue";
import SettingsPanel from "../lib/components/SettingsPanel.vue";
import Sidebar from "../lib/components/Sidebar.vue";
import { filterHosts, needsAttention, sortHosts, tagCounts, type Section } from "../lib/filter";
import { applyTheme } from "../lib/theme";
import type { Host, HostList, HostStatus } from "../lib/types";

const hosts = ref<Host[]>([]);
const warnings = ref<string[]>([]);
const statuses = ref<Map<string, HostStatus>>(new Map());
const query = ref("");
const section = ref<Section>("all");
const tag = ref<string | null>(null);
const selectedId = ref<string | null>(null);
const editing = ref<Host | null | undefined>(undefined);
const showSettings = ref(false);
const checking = ref(false);
const toast = ref<string | null>(null);
const loadError = ref<string | null>(null);
const searchInput = ref<HTMLInputElement | null>(null);
const unlisteners: Array<() => void> = [];
let toastTimer: number | undefined;

const offline = computed(() => {
	const names = new Set<string>();
	for (const status of statuses.value.values()) {
		if (status.state === "offline") {
			names.add(status.name);
		}
	}

	return names;
});

const visibleHosts = computed(() => sortHosts(filterHosts(
	hosts.value,
	{ query: query.value, tag: tag.value, section: section.value },
	offline.value,
)));
const selected = computed(() => hosts.value.find((host) => host.id === selectedId.value) ?? null);
const tags = computed(() => tagCounts(hosts.value));
const favouriteCount = computed(() => hosts.value.filter((host) => host.meta.favourite).length);
const attentionCount = computed(() => hosts.value
	.filter((host) => needsAttention(host, offline.value))
	.length);

function showToast(message: string): void {
	toast.value = message;
	window.clearTimeout(toastTimer);
	toastTimer = window.setTimeout(() => {
		toast.value = null;
	}, 6000);
}

function applyList(list: HostList): void {
	hosts.value = list.hosts;
	warnings.value = list.warnings;
	if (selectedId.value !== null && !list.hosts.some((host) => host.id === selectedId.value)) {
		selectedId.value = null;
	}
}

async function reload(): Promise<void> {
	try {
		applyList(await listHosts());
		loadError.value = null;
	}
	catch (error) {
		loadError.value = errorMessage(error);
	}
}

async function runStatusChecks(): Promise<void> {
	const names = hosts.value
		.filter((host) => host.editable)
		.map((host) => host.name);
	if (names.length === 0 || checking.value) {
		return;
	}

	checking.value = true;
	try {
		const results = await checkStatus(names);
		statuses.value = new Map(results.map((status) => [status.name, status]));
	}
	catch (error) {
		showToast(errorMessage(error));
	}
	finally {
		checking.value = false;
	}
}

async function connectTo(host: Host, forward: number | null): Promise<void> {
	try {
		await connect(host.name, forward);
		await reload();
	}
	catch (error) {
		showToast(errorMessage(error));
	}
}

async function toggleFavourite(host: Host, value: boolean): Promise<void> {
	try {
		applyList(await setFavourite(host.name, value));
	}
	catch (error) {
		showToast(errorMessage(error));
	}
}

async function remove(host: Host): Promise<void> {
	const question = `Delete ${host.name} from your SSH config? A backup is made first.`;
	if (!window.confirm(question)) {
		return;
	}

	try {
		applyList(await deleteHost(host.name));
		selectedId.value = null;
		showToast(`Deleted ${host.name}.`);
	}
	catch (error) {
		showToast(errorMessage(error));
	}
}

function saved(list: HostList): void {
	applyList(list);
	editing.value = undefined;
}

async function focusSearch(): Promise<void> {
	await nextTick();
	searchInput.value?.focus();
	searchInput.value?.select();
}

// Enter in the search box connects to the first match, so quick launch needs no mouse.
function connectFirstMatch(): void {
	const first = visibleHosts.value.find((host) => host.editable);
	if (first) {
		void connectTo(first, null);
	}
}

async function loadTheme(): Promise<void> {
	try {
		applyTheme((await getSettings()).settings.theme);
	}
	catch (error) {
		showToast(errorMessage(error));
	}
}

onMounted(async () => {
	unlisteners.push(await onHostsChanged(() => void reload()));
	unlisteners.push(await onQuickLaunch(() => void focusSearch()));
	unlisteners.push(await onError((message) => showToast(message)));
	await Promise.all([reload(), loadTheme()]);
	void focusSearch();
	void runStatusChecks();
});

onUnmounted(() => {
	for (const unlisten of unlisteners) {
		unlisten();
	}

	window.clearTimeout(toastTimer);
});
</script>

<template>
	<div class="layout">
		<Sidebar
			:section="section"
			:tag="tag"
			:tags="tags"
			:total-count="hosts.length"
			:favourite-count="favouriteCount"
			:attention-count="attentionCount"
			@section="section = $event"
			@tag="tag = $event"
			@settings="showSettings = true"
		/>

		<main class="main">
			<div class="toolbar">
				<input
					ref="searchInput"
					v-model="query"
					class="search"
					type="search"
					placeholder="Search hosts, addresses, tags… (Enter connects to the first)"
					aria-label="Search hosts"
					spellcheck="false"
					@keydown.enter="connectFirstMatch"
					@keydown.esc="query = ''"
				>
				<button
					class="button"
					:disabled="checking"
					@click="runStatusChecks"
				>
					{{ checking ? "Checking…" : "Check status" }}
				</button>
				<button
					class="button button--primary"
					@click="editing = null"
				>
					Add host
				</button>
			</div>

			<p
				v-if="loadError"
				class="banner banner--error"
				role="alert"
			>
				{{ loadError }}
			</p>
			<p
				v-for="warning in warnings"
				:key="warning"
				class="banner"
			>
				{{ warning }}
			</p>

			<div
				v-if="visibleHosts.length > 0"
				class="card-grid"
			>
				<HostCard
					v-for="host in visibleHosts"
					:key="host.id"
					:host="host"
					:status="statuses.get(host.name)"
					:selected="host.id === selectedId"
					@select="selectedId = host.id"
					@connect="connectTo(host, $event)"
					@favourite="toggleFavourite(host, $event)"
				/>
			</div>
			<div
				v-else-if="!loadError"
				class="empty"
			>
				<p v-if="hosts.length === 0">
					No hosts in your SSH config yet.
				</p>
				<p v-else>
					No hosts match.
				</p>
			</div>
		</main>

		<HostDetail
			v-if="selected"
			:host="selected"
			:status="statuses.get(selected.name)"
			@close="selectedId = null"
			@edit="editing = selected"
			@delete="remove(selected)"
			@connect="connectTo(selected, $event)"
		/>

		<AddHostForm
			v-if="editing !== undefined"
			:host="editing"
			@saved="saved"
			@cancel="editing = undefined"
		/>

		<SettingsPanel
			v-if="showSettings"
			@close="showSettings = false"
			@saved="applyTheme($event.theme); reload()"
		/>

		<p
			v-if="toast"
			class="toast"
			role="status"
		>
			{{ toast }}
		</p>
	</div>
</template>
