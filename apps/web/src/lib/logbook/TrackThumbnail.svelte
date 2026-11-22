<!--
	A small drawing of a flight's shape, so flights can be told apart in a list
	at a glance. It is decoration: the row beside it says which flight it is.

	`large` is the pilot's own list's: a little map of its own, a quiet grid
	of the pack's hairline under the track, north up, with the take-off as a
	hollow dot and the landing as a solid one, as the flight's map has them.
-->
<script lang="ts">
	import type { Coordinate } from '#lib/core/index.ts';
	import { outlinePoints } from './track-shape';

	interface Props {
		outline: Coordinate[];
		large?: boolean;
	}

	let { outline, large = false }: Props = $props();

	/** The drawing is 52 units square whatever size it is shown at. */
	const SIZE = 52;
	const points = $derived(outlinePoints(outline, SIZE, SIZE, large ? 7 : 8));
	/** Where the track begins and ends, in the drawing's units. */
	const ends = $derived.by(() => {
		const all = points?.split(' ');
		if (!all || all.length < 2) return undefined;
		const at = (point: string) => point.split(',').map(Number) as [number, number];
		return { first: at(all[0]), last: at(all[all.length - 1]) };
	});
	/** The grid's lines, every 13 units. */
	const GRID = [13, 26, 39];
</script>

<svg class:large viewBox="0 0 {SIZE} {SIZE}" aria-hidden="true" focusable="false">
	{#if large}
		<g class="grid">
			{#each GRID as at (at)}
				<line x1={at} y1="0" x2={at} y2={SIZE} />
				<line x1="0" y1={at} x2={SIZE} y2={at} />
			{/each}
		</g>
	{/if}
	{#if points}
		<polyline {points} />
		{#if large && ends}
			<circle class="start" cx={ends.first[0]} cy={ends.first[1]} r="2.25" />
			<circle class="end" cx={ends.last[0]} cy={ends.last[1]} r="2.25" />
		{/if}
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

	svg.large {
		width: 4.5rem;
		height: 4.5rem;
		border-radius: var(--radius-card);
		background: var(--background);
		box-shadow: inset 0 0 0 1px var(--separator);
	}

	.grid line {
		stroke: var(--separator);
		stroke-width: 0.5;
	}

	polyline {
		fill: none;
		stroke: var(--accent);
		stroke-width: 2;
		stroke-linecap: round;
		stroke-linejoin: round;
	}

	.large polyline {
		stroke-width: 1.75;
	}

	circle {
		stroke: var(--text);
		stroke-width: 1;
	}

	.start {
		fill: var(--surface);
	}

	.end {
		fill: var(--text);
	}
</style>
