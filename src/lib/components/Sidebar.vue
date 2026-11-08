<script setup lang="ts">
import type { Section } from "../filter";

defineProps<{
	section: Section;
	selectedTags: readonly string[];
	tags: Array<[string, number]>;
	totalCount: number;
	favouriteCount: number;
	attentionCount: number;
}>();

const emit = defineEmits<{
	section: [section: Section];
	tag: [tag: string];
	clearTags: [];
	settings: [];
}>();
</script>

<template>
	<nav
		class="sidebar"
		aria-label="Host filters"
	>
		<h1 class="sidebar__title">
			Hop
		</h1>

		<ul class="nav-list">
			<li>
				<button
					class="nav-item"
					:class="{ 'nav-item--active': section === 'all' && selectedTags.length === 0 }"
					@click="emit('section', 'all'); emit('clearTags')"
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
			<div class="sidebar__heading-row">
				<h2
					id="tag-heading"
					class="sidebar__heading"
				>
					Tags
				</h2>
				<button
					v-if="selectedTags.length > 0"
					class="link-button"
					@click="emit('clearTags')"
				>
					Clear
				</button>
			</div>
			<p
				v-if="selectedTags.length > 1"
				class="sidebar__hint"
			>
				Showing hosts with all selected tags.
			</p>
			<ul
				class="nav-list"
				aria-labelledby="tag-heading"
			>
				<li
					v-for="[name, count] in tags"
					:key="name"
				>
					<button
						class="nav-item"
						:class="{ 'nav-item--active': selectedTags.includes(name) }"
						:aria-pressed="selectedTags.includes(name)"
						:aria-label="`${name}, ${count} ${count === 1 ? 'host' : 'hosts'}`"
						@click="emit('tag', name)"
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
