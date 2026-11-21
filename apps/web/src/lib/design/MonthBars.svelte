<!--
	The twelve months as a small row of bars: when in the year flights were
	flown, from a take-off or by a pilot. Each bar is as tall as the core's
	share of the busiest month, in the primary colour (a count, never good
	or bad), with the core's short name under it and the core's words for a
	screen reader.
-->
<script lang="ts">
	import type { FlownMonth } from '#lib/core/index.ts';

	interface Props {
		/** The months, January first, as the core gives them. */
		months: readonly FlownMonth[];
	}

	let { months }: Props = $props();
</script>

<ul class="bars">
	{#each months as month (month.name)}
		<li>
			<span class="track" aria-hidden="true">
				<span class="bar" style:height="{month.share * 100}%"></span>
			</span>
			<span class="name" aria-hidden="true">{month.name}</span>
			<span class="visually-hidden">{month.words}</span>
		</li>
	{/each}
</ul>

<style>
	.bars {
		display: flex;
		gap: var(--space-1);
		list-style: none;
	}

	.bars li {
		display: flex;
		flex: 1;
		flex-direction: column;
		align-items: center;
		gap: var(--space-1);
		min-width: 0;
	}

	/* A bar as tall as the core's share of the busiest month, standing on the
	   pack's faint fill: a count, never good or bad. */
	.track {
		display: flex;
		align-items: flex-end;
		width: 100%;
		height: var(--target);
		border-radius: var(--radius-s);
		background: var(--fill);
		overflow: hidden;
	}

	.bar {
		width: 100%;
		background: var(--accent);
	}

	.name {
		font-size: var(--caption);
		color: var(--text-secondary);
	}
</style>
