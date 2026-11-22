<!--
	The map's pin for a flying site, drawn for a list or a heading: dark with
	the pack's take-off icon in cream for a place to take off from, cream
	with its landing icon in the dark for a landing, a ring for the rest, and muted for one that stands apart from
	those a filter leaves. It looks the same in both appearances, as
	the map does. It only repeats what the text beside it says, so a screen
	reader passes over it.
-->
<script lang="ts">
	import type { SiteKind } from '#lib/core/index.ts';
	import {
		colours,
		EDGE_WIDTH,
		ICON_AT,
		ICON_SCALE,
		iconOf,
		MARK,
		PIN,
		PIN_HEIGHT,
		PIN_WIDTH,
		RING_WIDTH,
		ringed
	} from './symbols';

	interface Props {
		kind: SiteKind;
		/** Muted, for a take-off whose source does not say what the filter asks: as on the map. */
		apart?: boolean;
	}

	let { kind, apart = false }: Props = $props();

	const look = $derived(colours(kind, apart));
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
		<g
			transform="translate({ICON_AT.x} {ICON_AT.y}) scale({ICON_SCALE})"
			fill="none"
			stroke={look.sign}
			stroke-width="2"
			stroke-linecap="round"
			stroke-linejoin="round"
		>
			{#each iconOf(kind) as shape, index (index)}
				{#if shape.kind === 'path'}
					<path d={shape.d} />
				{:else}
					<ellipse cx={shape.cx} cy={shape.cy} rx={shape.rx} ry={shape.ry} />
				{/if}
			{/each}
		</g>
	{/if}
</svg>

<style>
	svg {
		flex: none;
		display: block;
	}
</style>
