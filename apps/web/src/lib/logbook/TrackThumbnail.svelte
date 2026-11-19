<!--
	A small drawing of a flight's shape, so flights can be told apart in a list
	at a glance. It is decoration: the row beside it says which flight it is.
-->
<script lang="ts">
	import type { Coordinate } from '#lib/core/index.ts';
	import { outlinePoints } from './track-shape';

	interface Props {
		outline: Coordinate[];
	}

	let { outline }: Props = $props();

	/** The drawing is 52 units square whatever size it is shown at. */
	const SIZE = 52;
	const points = $derived(outlinePoints(outline, SIZE, SIZE, 8));
</script>

<svg viewBox="0 0 {SIZE} {SIZE}" aria-hidden="true" focusable="false">
	{#if points}
		<polyline {points} />
	{/if}
</svg>

<style>
	svg {
		display: block;
		flex: none;
		width: 3.25rem;
		height: 3.25rem;
		border-radius: 0.75rem;
		background: var(--fill);
	}

	polyline {
		fill: none;
		stroke: var(--accent);
		stroke-width: 2;
		stroke-linecap: round;
		stroke-linejoin: round;
	}
</style>
