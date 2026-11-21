<!--
	What the filter of the flying sites is and what it leaves, in the core's
	words: "Take-offs for a wind from NW", "12 of 48 take-offs fit a wind
	from NW", and how many take-offs stand apart because their source does
	not say ("31 take-offs give no wind directions"). It stands with the
	filter's choices, on the map while a filter is on, and over the list.
-->
<script lang="ts">
	import type { SitesFiltered } from '#lib/core/index.ts';

	interface Props {
		left: SitesFiltered;
		/**
		 * Whether there are sites to count: not while the map is too far out
		 * to hold any, where "No take-offs here" would say what nobody knows.
		 */
		counted?: boolean;
	}

	let { left, counted = true }: Props = $props();
</script>

<div class="line">
	<p class="title">{left.filterWords}</p>
	{#if counted}
		<p class="numeric">{left.words}</p>
		{#if left.unsaidTitle}
			<p class="numeric">{left.unsaidTitle}</p>
		{/if}
	{/if}
</div>

<style>
	.line {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		min-width: 0;
		text-align: left;
	}

	p {
		margin: 0;
		color: var(--text-secondary);
		font-size: var(--caption);
	}

	.title {
		color: var(--text);
		font-size: var(--subheadline);
		font-weight: 600;
	}
</style>
