<script setup lang="ts">
import { computed } from "vue";

import { relativeTime } from "../time";
import { findTunnel, forwardName, tunnelTitle } from "../tunnels";
import type { Host, HostStatus, TunnelView } from "../types";

const props = defineProps<{
	host: Host;
	status?: HostStatus;
	selected: boolean;
	tunnels: TunnelView[];
}>();

const emit = defineEmits<{
	connect: [forward: number | null];
	tunnel: [forward: number];
	select: [];
	favourite: [value: boolean];
}>();

function tunnelAt(index: number): TunnelView | null {
	return findTunnel(props.tunnels, props.host.name, index);
}

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
const cardLabel = computed(() => {
	const state = props.host.editable ? `, ${statusLabel.value.toLowerCase()}` : ", read-only";
	return `${props.host.name}${state}`;
});
const sourceName = computed(() => props.host.source.split(/[\\/]/).pop() ?? props.host.source);
</script>

<template>
	<article
		class="host-card"
		:class="{ 'host-card--selected': selected, 'host-card--readonly': !host.editable }"
		role="listitem"
		:aria-label="cardLabel"
		:aria-current="selected ? 'true' : undefined"
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
				class="button button--small tunnel-button"
				:class="`tunnel-button--${tunnelAt(index)?.state ?? 'idle'}`"
				:title="tunnelTitle(forward, tunnelAt(index))"
				@click.stop="emit('tunnel', index)"
			>
				{{ forwardName(forward) }}
			</button>
		</div>
	</article>
</template>
