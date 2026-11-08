<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";

import {
	checkStatus,
	connect,
	deleteHost,
	errorMessage,
	getSettings,
	listHosts,
	listTunnels,
	onError,
	onHostsChanged,
	onQuickLaunch,
	onTunnelsChanged,
	setFavourite,
	startTunnel,
	stopTunnel,
} from "../lib/api";
import AddHostForm from "../lib/components/AddHostForm.vue";
import HostCard from "../lib/components/HostCard.vue";
import HostDetail from "../lib/components/HostDetail.vue";
import SettingsPanel from "../lib/components/SettingsPanel.vue";
import Sidebar from "../lib/components/Sidebar.vue";
import {
	filterHosts,
	needsAttention,
	sortHosts,
	tagCounts,
	toggleTag,
	type Section,
} from "../lib/filter";
import {
	columnCount,
	isGridKey,
	isSearchShortcut,
	isTypingTarget,
	moveInGrid,
} from "../lib/keyboard";
import { applyTheme } from "../lib/theme";
import { findTunnel, forwardName, tunnelAction } from "../lib/tunnels";
import type { Host, HostList, HostStatus, Settings, TunnelView } from "../lib/types";

const hosts = ref<Host[]>([]);
const warnings = ref<string[]>([]);
const statuses = ref<Map<string, HostStatus>>(new Map());
const tunnels = ref<TunnelView[]>([]);
const query = ref("");
const section = ref<Section>("all");
const selectedTags = ref<string[]>([]);
const focusIndex = ref(0);
const grid = ref<HTMLElement | null>(null);
const selectedId = ref<string | null>(null);
const editing = ref<Host | null | undefined>(undefined);
const showSettings = ref(false);
const checking = ref(false);
const autoStatusChecks = ref(true);
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
	{ query: query.value, tags: selectedTags.value, section: section.value },
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

async function checkOne(host: Host): Promise<void> {
	try {
		const [result] = await checkStatus([host.name]);
		if (result) {
			statuses.value = new Map(statuses.value).set(result.name, result);
		}
	}
	catch (error) {
		showToast(errorMessage(error));
	}
}

function settingsSaved(settings: Settings): void {
	applyTheme(settings.theme);
	autoStatusChecks.value = settings.autoStatusChecks;
	// Results from before checks were turned off would look current, so drop them.
	if (!settings.autoStatusChecks) {
		statuses.value = new Map();
	}

	void reload();
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

async function reloadTunnels(): Promise<void> {
	try {
		tunnels.value = await listTunnels();
	}
	catch (error) {
		showToast(errorMessage(error));
	}
}

async function toggleTunnel(host: Host, index: number): Promise<void> {
	const current = findTunnel(tunnels.value, host.name, index);
	try {
		if (current && tunnelAction(current) === "stop") {
			tunnels.value = await stopTunnel(current.localPort);
			return;
		}

		tunnels.value = await startTunnel(host.name, index);
		const started = findTunnel(tunnels.value, host.name, index);
		const forward = host.meta.forwards[index];
		if (started?.state === "failed") {
			showToast(started.error ?? "The tunnel stopped straight away.");
		}
		else if (forward) {
			showToast(`${forwardName(forward)} is open on localhost:${forward.localPort}.`);
		}
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

function clearFilters(): void {
	query.value = "";
	selectedTags.value = [];
}

function cardElements(): HTMLElement[] {
	return Array.from(grid.value?.querySelectorAll<HTMLElement>(".host-card") ?? []);
}

// Columns come from the live layout because the grid reflows with the window width.
function gridColumns(): number {
	if (!grid.value) {
		return 1;
	}

	return columnCount(window.getComputedStyle(grid.value).gridTemplateColumns);
}

function focusCard(index: number): void {
	const cards = cardElements();
	const card = cards[index];
	if (card) {
		focusIndex.value = index;
		card.focus();
	}
}

function onGridKeydown(event: KeyboardEvent): void {
	// Keys aimed at a button inside a card (favourite, tunnels) keep their usual meaning.
	const target = event.target as HTMLElement | null;
	const cards = cardElements();
	const index = target ? cards.indexOf(target) : -1;
	if (index < 0) {
		return;
	}

	if (isGridKey(event.key)) {
		event.preventDefault();
		focusCard(moveInGrid(index, cards.length, gridColumns(), event.key));
	}
	else if (event.key === "Enter") {
		event.preventDefault();
		const host = visibleHosts.value[index];
		if (host?.editable) {
			void connectTo(host, null);
		}
	}
	else if (event.key === " ") {
		event.preventDefault();
		const host = visibleHosts.value[index];
		if (host) {
			selectedId.value = host.id;
		}
	}
}

function onWindowKeydown(event: KeyboardEvent): void {
	const typing = isTypingTarget(event.target as HTMLElement | null);
	if (isSearchShortcut(event, typing)) {
		event.preventDefault();
		void focusSearch();
		return;
	}

	// The form and settings dialogs close themselves on Esc, and text fields keep the key.
	if (event.key !== "Escape" || typing || editing.value !== undefined || showSettings.value) {
		return;
	}

	if (selectedId.value !== null) {
		selectedId.value = null;
	}
	else {
		clearFilters();
	}
}

async function loadSettings(): Promise<void> {
	try {
		const { settings } = await getSettings();
		applyTheme(settings.theme);
		autoStatusChecks.value = settings.autoStatusChecks;
	}
	catch (error) {
		showToast(errorMessage(error));
	}
}

watch(visibleHosts, (list) => {
	if (focusIndex.value >= list.length) {
		focusIndex.value = Math.max(0, list.length - 1);
	}
});

onMounted(async () => {
	window.addEventListener("keydown", onWindowKeydown);
	unlisteners.push(await onHostsChanged(() => void reload()));
	unlisteners.push(await onQuickLaunch(() => void focusSearch()));
	unlisteners.push(await onError((message) => showToast(message)));
	unlisteners.push(await onTunnelsChanged(() => void reloadTunnels()));
	await Promise.all([reload(), loadSettings(), reloadTunnels()]);
	void focusSearch();
	if (autoStatusChecks.value) {
		void runStatusChecks();
	}
});

onUnmounted(() => {
	window.removeEventListener("keydown", onWindowKeydown);
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
			:selected-tags="selectedTags"
			:tags="tags"
			:total-count="hosts.length"
			:favourite-count="favouriteCount"
			:attention-count="attentionCount"
			@section="section = $event"
			@tag="selectedTags = toggleTag(selectedTags, $event)"
			@clear-tags="selectedTags = []"
			@settings="showSettings = true"
		/>

		<main class="main">
			<div class="toolbar">
				<input
					ref="searchInput"
					v-model="query"
					class="search"
					type="search"
					placeholder="Search hosts, addresses, tags… (/ to focus, Enter connects)"
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
					{{ checking ? "Checking…" : "Check now" }}
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
				ref="grid"
				class="card-grid"
				role="list"
				aria-label="Hosts. Use the arrow keys to move, Enter to connect."
				@keydown="onGridKeydown"
			>
				<HostCard
					v-for="(host, index) in visibleHosts"
					:key="host.id"
					:host="host"
					:tabindex="index === focusIndex ? 0 : -1"
					:status="statuses.get(host.name)"
					:selected="host.id === selectedId"
					:tunnels="tunnels"
					@select="selectedId = host.id"
					@focus="focusIndex = index"
					@connect="connectTo(host, $event)"
					@tunnel="toggleTunnel(host, $event)"
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
			:auto-status-checks="autoStatusChecks"
			:tunnels="tunnels"
			@check="checkOne(selected)"
			@close="selectedId = null"
			@edit="editing = selected"
			@delete="remove(selected)"
			@connect="connectTo(selected, $event)"
			@tunnel="toggleTunnel(selected, $event)"
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
			@saved="settingsSaved"
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
