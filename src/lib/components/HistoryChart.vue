<script setup lang="ts">
import { computed } from "vue";

import { dailyCounts } from "../time";
import type { HistoryEntry } from "../types";

const props = defineProps<{
	entries: HistoryEntry[];
	days: number;
}>();

const WIDTH = 300;
const HEIGHT = 60;

const buckets = computed(() => dailyCounts(
	props.entries.map((entry) => entry.startedAt),
	props.days,
));
const peak = computed(() => Math.max(1, ...buckets.value.map((bucket) => bucket.count)));
const barWidth = computed(() => WIDTH / props.days);
const total = computed(() => buckets.value.reduce((sum, bucket) => sum + bucket.count, 0));

function barHeight(count: number): number {
	if (count === 0) {
		return 1;
	}

	return Math.max(3, (count / peak.value) * HEIGHT);
}

function label(day: Date, count: number): string {
	const date = day.toLocaleDateString("en-GB", { day: "numeric", month: "short" });
	return `${date}: ${count} ${count === 1 ? "connection" : "connections"}`;
}
</script>

<template>
	<figure class="history-chart">
		<svg
			:viewBox="`0 0 ${WIDTH} ${HEIGHT}`"
			preserveAspectRatio="none"
			role="img"
			:aria-label="`${total} connections in the last ${days} days`"
		>
			<rect
				v-for="(bucket, index) in buckets"
				:key="index"
				:x="index * barWidth + 1"
				:y="HEIGHT - barHeight(bucket.count)"
				:width="barWidth - 2"
				:height="barHeight(bucket.count)"
				:class="bucket.count > 0 ? 'history-chart__bar' : 'history-chart__empty'"
			>
				<title>{{ label(bucket.day, bucket.count) }}</title>
			</rect>
		</svg>
		<figcaption class="muted">
			{{ total }} {{ total === 1 ? "connection" : "connections" }} in the last {{ days }} days
		</figcaption>
	</figure>
</template>
