<!--
	The foehn view: each pair of stations with its words, its level, how old
	its reading is and its last 24 hours, then what the number means, what
	it does not say, and whose data it is. Every word is the core's
	(`FoehnView`, `crates/core/src/foehn.rs`); the view writes none.
-->
<script lang="ts">
	import type { FoehnView } from '#lib/core/index.ts';
	import FoehnBadge from './FoehnBadge.svelte';
	import FoehnChart from './FoehnChart.svelte';

	interface Props {
		view: FoehnView;
	}

	let { view }: Props = $props();
</script>

<div class="foehn-view">
	{#if view.rows.length === 0}
		<p class="empty">{view.empty}</p>
	{/if}
	{#each view.rows as row (row.id)}
		<section class="pair" aria-label={row.title} data-testid="foehn-row">
			<header>
				<h3>{row.title}</h3>
				<FoehnBadge level={row.level} label={row.levelLabel} />
			</header>
			<p class="words">{row.words}</p>
			{#if row.when}
				<p class="when numeric">{row.when}</p>
			{/if}
			{#if row.points.length > 0}
				<FoehnChart
					points={row.points}
					limits={view.limits}
					title={view.chartTitle}
					unit={view.unit}
					above={row.aboveLabel}
					below={row.belowLabel}
				/>
			{/if}
			{#if row.note}
				<p class="small">{row.note}</p>
			{/if}
		</section>
	{/each}
	<p class="about">{view.explanation}</p>
	<p class="about">{view.caution}</p>
	{#each view.credits as credit (credit.source)}
		<p class="small">
			<a href={credit.url} target="_blank" rel="noopener noreferrer">{credit.text}</a>
		</p>
	{/each}
</div>

<style>
	.foehn-view {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}

	.pair {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding-bottom: var(--space-4);
		box-shadow: inset 0 -1px 0 var(--separator);
	}

	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
	}

	h3 {
		font-size: var(--title);
		font-weight: 600;
	}

	.when,
	.small,
	.empty {
		color: var(--text-secondary);
	}

	.when {
		font-size: var(--subheadline);
	}

	.small {
		font-size: var(--caption);
	}

	.about {
		font-size: var(--subheadline);
	}

	a {
		color: var(--link);
	}
</style>
