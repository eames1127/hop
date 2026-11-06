<script setup lang="ts">
import type { Section } from "../filter";

defineProps<{
	section: Section;
	tag: string | null;
	tags: Array<[string, number]>;
	totalCount: number;
	favouriteCount: number;
	attentionCount: number;
}>();

const emit = defineEmits<{
	section: [section: Section];
	tag: [tag: string | null];
	settings: [];
}>();
</script>

<template>
	<nav class="sidebar">
		<h1 class="sidebar__title">
			Hop
		</h1>

		<ul class="nav-list">
			<li>
				<button
					class="nav-item"
					:class="{ 'nav-item--active': section === 'all' && tag === null }"
					@click="emit('section', 'all'); emit('tag', null)"
				>
					<span>All hosts</span>
					<span class="count">{{ totalCount }}</span>
				</button>
			</li>
			<li>
				<button
					class="nav-item"
					:class="{ 'nav-item--active': section === 'favourites' }"
					@click="emit('section', 'favourites')"
				>
					<span>Favourites</span>
					<span class="count">{{ favouriteCount }}</span>
				</button>
			</li>
			<li>
				<button
					class="nav-item"
					:class="{ 'nav-item--active': section === 'attention' }"
					@click="emit('section', 'attention')"
				>
					<span>Needs attention</span>
					<span
						class="count"
						:class="{ 'count--alert': attentionCount > 0 }"
					>{{ attentionCount }}</span>
				</button>
			</li>
		</ul>

		<template v-if="tags.length > 0">
			<h2 class="sidebar__heading">
				Tags
			</h2>
			<ul class="nav-list">
				<li
					v-for="[name, count] in tags"
					:key="name"
				>
					<button
						class="nav-item"
						:class="{ 'nav-item--active': tag === name }"
						@click="emit('tag', tag === name ? null : name)"
					>
						<span>{{ name }}</span>
						<span class="count">{{ count }}</span>
					</button>
				</li>
			</ul>
		</template>

		<span class="spacer" />
		<button
			class="nav-item"
			@click="emit('settings')"
		>
			Settings
		</button>
	</nav>
</template>
