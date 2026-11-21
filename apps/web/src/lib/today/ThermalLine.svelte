<!--
	The thermals of a day at a take-off: the kind of day beside its mark,
	and under it the figures in a line, as the iPhone's `ThermalLine`. Both
	are the core's estimate from the weather model, by fixed limits.
-->
<script lang="ts">
	import type { OutlookDay } from '#lib/core/index.ts';
	import ThermalMark from './ThermalMark.svelte';

	interface Props {
		day: Pick<OutlookDay, 'thermalWords' | 'thermalLine'> & { climbMs?: number };
		/** The mark's colour, where it stands for a square of the map. */
		color?: string;
	}

	let { day, color }: Props = $props();
</script>

<p class="thermals">
	<ThermalMark climbMs={day.climbMs ?? 0} {color} />
	<span class="words">
		<span>{day.thermalWords}</span>
		<span class="line">{day.thermalLine}</span>
	</span>
</p>

<style>
	.thermals {
		display: flex;
		align-items: flex-start;
		gap: var(--space-2);
	}

	/* Beside the first line's letters, not above them. */
	.thermals :global(.mark) {
		margin-top: var(--space-1);
	}

	.words {
		display: flex;
		flex-direction: column;
	}

	.line {
		color: var(--text-secondary);
	}
</style>
