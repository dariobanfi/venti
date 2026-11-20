<!--
	The map's picture of another pilot's glider, drawn for a list: the same
	wing seen from above, green while it flies and grey when nothing was heard
	of it for a while, or the grey disc of a glider on the ground. Like on the
	map it has a white edge. It only repeats what the text beside it says, so
	a screen reader passes over it.
-->
<script lang="ts">
	import type { LiveGlider } from '#lib/core/index.ts';
	import { colourOf, DISC_RADIUS, lookOf, PILOT, SIDE, WING } from './symbols';

	let { glider }: { glider: LiveGlider } = $props();

	const look = $derived(lookOf(glider));
</script>

<svg
	viewBox="0 0 {SIDE} {SIDE}"
	width="1.75rem"
	height="1.75rem"
	aria-hidden="true"
	focusable="false"
>
	{#if look === 'landed'}
		<circle cx={SIDE / 2} cy={SIDE / 2} r={DISC_RADIUS} fill={colourOf(look)} />
	{:else}
		<path d={WING} fill={colourOf(look)} />
		<circle cx={PILOT.x} cy={PILOT.y} r={PILOT.radius} fill={colourOf(look)} />
	{/if}
</svg>

<style>
	svg {
		flex: none;
		display: block;
	}

	path,
	circle {
		stroke: #ffffff;
		stroke-width: 2.5;
		stroke-linejoin: round;
		/* The edge lies under the shape, so the shape keeps its size. */
		paint-order: stroke;
	}
</style>
