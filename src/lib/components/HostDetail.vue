<script setup lang="ts">
import { ref, watch } from "vue";

import { errorMessage, hostDetail } from "../api";
import { relativeTime } from "../time";
import type { Host, HostDetail, HostStatus } from "../types";
import HistoryChart from "./HistoryChart.vue";

const props = defineProps<{
	host: Host;
	status?: HostStatus;
}>();

const emit = defineEmits<{
	close: [];
	edit: [];
	delete: [];
	connect: [forward: number | null];
}>();

const detail = ref<HostDetail | null>(null);
const loadError = ref<string | null>(null);
const loading = ref(false);

async function load(): Promise<void> {
	detail.value = null;
	loadError.value = null;
	if (!props.host.editable) {
		return;
	}

	loading.value = true;
	try {
		detail.value = await hostDetail(props.host.name);
	}
	catch (error) {
		loadError.value = errorMessage(error);
	}
	finally {
		loading.value = false;
	}
}

watch(
	() => [props.host.name, props.host.lastUsed],
	() => {
		void load();
	},
	{ immediate: true },
);
</script>

<template>
	<aside class="detail">
		<header class="detail__header">
			<h2>{{ host.name }}</h2>
			<button
				class="icon-button"
				aria-label="Close details"
				@click="emit('close')"
			>
				✕
			</button>
		</header>

		<p
			v-if="host.meta.description"
			class="detail__description"
		>
			{{ host.meta.description }}
		</p>

		<div
			v-if="host.editable"
			class="detail__actions"
		>
			<button
				class="button button--primary"
				@click="emit('connect', null)"
			>
				Connect
			</button>
			<button
				class="button"
				@click="emit('edit')"
			>
				Edit
			</button>
			<button
				class="button button--danger"
				@click="emit('delete')"
			>
				Delete
			</button>
		</div>
		<p
			v-else
			class="notice"
		>
			This block has wildcards, several aliases or a Match rule, so Hop shows it but never
			rewrites it. Edit it by hand in your config.
		</p>

		<section class="detail__section">
			<h3>Status</h3>
			<p v-if="!status">
				Not checked yet.
			</p>
			<p v-else-if="status.state === 'online'">
				Online<span v-if="status.latencyMs !== null"> ({{ status.latencyMs }} ms)</span>
			</p>
			<p v-else>
				{{ status.state === "offline" ? "Offline" : "Unknown" }}: {{ status.detail }}
			</p>
		</section>

		<section class="detail__section">
			<h3>Final settings</h3>
			<p
				v-if="loading"
				class="muted"
			>
				Asking OpenSSH…
			</p>
			<p
				v-else-if="loadError || detail?.error"
				class="error-text"
			>
				{{ loadError ?? detail?.error }}
			</p>
			<dl
				v-else-if="detail?.resolved"
				class="detail__grid"
			>
				<dt>Address</dt>
				<dd>{{ detail.resolved.hostName }}</dd>
				<dt>User</dt>
				<dd>{{ detail.resolved.user ?? "—" }}</dd>
				<dt>Port</dt>
				<dd>{{ detail.resolved.port }}</dd>
				<dt>Jump host</dt>
				<dd>{{ detail.resolved.proxyJump ?? "—" }}</dd>
			</dl>
			<p
				v-else-if="!host.editable"
				class="muted"
			>
				Only shown for single hosts.
			</p>
			<p class="muted small">
				From <code>ssh -G</code>, after every wildcard and Include is applied.
			</p>
		</section>

		<section class="detail__section">
			<h3>Keys</h3>
			<p
				v-if="host.identityFiles.length === 0"
				class="muted"
			>
				No key set in this block; ssh uses its defaults and your agent.
			</p>
			<ul class="plain-list">
				<li
					v-for="path in host.identityFiles"
					:key="path"
				>
					<code>{{ path }}</code>
					<span
						v-if="host.missingKeys.includes(path)"
						class="flag flag--bad"
					>File not found</span>
				</li>
			</ul>
		</section>

		<section
			v-if="host.meta.forwards.length > 0"
			class="detail__section"
		>
			<h3>Port forwards</h3>
			<ul class="plain-list">
				<li
					v-for="(forward, index) in host.meta.forwards"
					:key="index"
				>
					<button
						class="button button--small"
						@click="emit('connect', index)"
					>
						{{ forward.label || "Forward" }}
					</button>
					<code>localhost:{{ forward.localPort }} → {{ forward.remoteHost }}:{{
						forward.remotePort }}</code>
				</li>
			</ul>
		</section>

		<section
			v-if="host.editable"
			class="detail__section"
		>
			<h3>History</h3>
			<p class="muted">
				Last used {{ relativeTime(host.lastUsed).toLowerCase() }} ·
				{{ host.useCount }} {{ host.useCount === 1 ? "connection" : "connections" }}
				in total
			</p>
			<HistoryChart
				v-if="detail"
				:entries="detail.history"
				:days="30"
			/>
		</section>

		<section class="detail__section">
			<h3>Source</h3>
			<p class="muted small">
				<code>{{ host.source }}</code>
			</p>
			<p
				v-if="host.meta.added"
				class="muted small"
			>
				Added {{ host.meta.added }}
			</p>
		</section>
	</aside>
</template>
