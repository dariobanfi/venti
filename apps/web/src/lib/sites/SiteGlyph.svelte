<!--
	The map's pin for a flying site, drawn for a list or a heading: dark with
	a cream dot for a place to take off from, cream with a dark dot for a
	landing, a ring for the rest. It looks the same in both appearances, as
	the map does. It only repeats what the text beside it says, so a screen
	reader passes over it.
-->
<script lang="ts">
	import type { SiteKind } from '#lib/core/index.ts';
	import {
		colours,
		EDGE_WIDTH,
		MARK,
		PIN,
		PIN_HEIGHT,
		PIN_WIDTH,
		RING_WIDTH,
		ringed
	} from './symbols';

	let { kind }: { kind: SiteKind } = $props();

	const look = $derived(colours(kind));
</script>

<svg
	viewBox="0 0 {PIN_WIDTH} {PIN_HEIGHT}"
	width="1.3125rem"
	height="1.75rem"
	aria-hidden="true"
	focusable="false"
>
	<path d={PIN} fill={look.ground} stroke={look.sign} stroke-width={EDGE_WIDTH} />
	{#if ringed(kind)}
		<circle
			cx={MARK.x}
			cy={MARK.y}
			r={MARK.radius}
			fill="none"
			stroke={look.sign}
			stroke-width={RING_WIDTH}
		/>
	{:else}
		<circle cx={MARK.x} cy={MARK.y} r={MARK.radius} fill={look.sign} />
	{/if}
</svg>

<style>
	svg {
		flex: none;
		display: block;
	}
</style>
