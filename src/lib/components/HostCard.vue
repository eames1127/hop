<script setup lang="ts">
import { computed } from "vue";

import { relativeTime } from "../time";
import type { Host, HostStatus } from "../types";

const props = defineProps<{
	host: Host;
	status?: HostStatus;
	selected: boolean;
}>();

const emit = defineEmits<{
	connect: [forward: number | null];
	select: [];
	favourite: [value: boolean];
}>();

const address = computed(() => {
	const host = props.host;
	if (host.kind === "match") {
		return "Conditional settings";
	}
	else if (!host.editable) {
		return "Shared settings for several hosts";
	}

	const user = host.user ? `${host.user}@` : "";
	const port = host.port ? `:${host.port}` : "";
	return `${user}${host.hostName ?? host.name}${port}`;
});

const statusLabel = computed(() => {
	if (!props.status) {
		return "Not checked";
	}
	else if (props.status.state === "online") {
		return "Online";
	}
	else if (props.status.state === "offline") {
		return "Offline";
	}

	return "Unknown";
});

const statusTitle = computed(() => props.status?.detail ?? statusLabel.value);
const sourceName = computed(() => props.host.source.split(/[\\/]/).pop() ?? props.host.source);
</script>

<template>
	<article
		class="host-card"
		:class="{ 'host-card--selected': selected, 'host-card--readonly': !host.editable }"
		@click="emit('select')"
	>
		<header class="host-card__header">
			<span
				v-if="host.editable"
				class="status-dot"
				:class="`status-dot--${status?.state ?? 'unchecked'}`"
				:title="statusTitle"
				:aria-label="statusLabel"
			/>
			<h3 class="host-card__name">
				{{ host.name }}
			</h3>
			<button
				v-if="host.editable"
				class="icon-button"
				:class="{ 'icon-button--on': host.meta.favourite }"
				:aria-label="host.meta.favourite ? 'Remove from favourites' : 'Add to favourites'"
				:title="host.meta.favourite ? 'Remove from favourites' : 'Add to favourites'"
				@click.stop="emit('favourite', !host.meta.favourite)"
			>
				{{ host.meta.favourite ? "★" : "☆" }}
			</button>
		</header>

		<p
			v-if="host.meta.description"
			class="host-card__description"
		>
			{{ host.meta.description }}
		</p>
		<p class="host-card__address">
			{{ address }}
		</p>

		<ul
			v-if="host.meta.tags.length > 0"
			class="tag-list"
		>
			<li
				v-for="tag in host.meta.tags"
				:key="tag"
				class="tag"
			>
				{{ tag }}
			</li>
		</ul>

		<ul
			v-if="host.stale || host.missingKeys.length > 0 || status?.state === 'offline'"
			class="flag-list"
		>
			<li
				v-if="status?.state === 'offline'"
				class="flag flag--bad"
			>
				Offline
			</li>
			<li
				v-if="host.missingKeys.length > 0"
				class="flag flag--bad"
			>
				Missing key
			</li>
			<li
				v-if="host.stale"
				class="flag flag--warn"
			>
				Stale
			</li>
		</ul>

		<footer class="host-card__footer">
			<span
				v-if="host.editable"
				class="muted"
			>
				Last used: {{ relativeTime(host.lastUsed) }}
			</span>
			<span
				v-else
				class="muted"
			>
				Read-only
			</span>
			<span
				v-if="host.included"
				class="muted"
				:title="host.source"
			>
				· {{ sourceName }}
			</span>
			<span class="spacer" />
			<button
				v-if="host.editable"
				class="button button--primary"
				@click.stop="emit('connect', null)"
			>
				Connect
			</button>
		</footer>

		<div
			v-if="host.editable && host.meta.forwards.length > 0"
			class="host-card__forwards"
		>
			<button
				v-for="(forward, index) in host.meta.forwards"
				:key="index"
				class="button button--small"
				:title="`ssh -L ${forward.localPort}:${forward.remoteHost}:${forward.remotePort}`"
				@click.stop="emit('connect', index)"
			>
				{{ forward.label || `Forward ${forward.localPort}` }}
			</button>
		</div>
	</article>
</template>
