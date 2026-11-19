<!--
	The map's symbol for a wind station, drawn for a list or a heading: the
	same dart, turned to where the wind blows and in the colour of its
	strength, or the dot of a station that is calm or has nothing current to
	say. Like on the map it has a white edge, without which the darkest blue
	is lost on a dark surface. It only repeats what the text beside it says,
	so a screen reader passes over it.
-->
<script lang="ts">
	import type { WindMark } from '#lib/core/index.ts';
	import { CALM_ON_MAP, WIND_BANDS } from '#lib/design/palette.ts';
	import { DART, DOT_RADIUS, SIDE } from './symbols';

	let { mark }: { mark: WindMark } = $props();

	const dart = DART.map(([x, y]) => `${x * SIDE},${y * SIDE}`).join(' ');
</script>

<svg
	class={mark.symbol.kind}
	viewBox="0 0 {SIDE} {SIDE}"
	width="1.75rem"
	height="1.75rem"
	aria-hidden="true"
	focusable="false"
>
	{#if mark.symbol.kind === 'arrow'}
		<polygon
			points={dart}
			fill={WIND_BANDS[mark.symbol.band].color}
			transform="rotate({mark.headingDeg} {SIDE / 2} {SIDE / 2})"
		/>
	{:else if mark.symbol.kind === 'calm'}
		<circle cx={SIDE / 2} cy={SIDE / 2} r={DOT_RADIUS} fill={CALM_ON_MAP} />
	{:else}
		<circle cx={SIDE / 2} cy={SIDE / 2} r={DOT_RADIUS} />
	{/if}
</svg>

<style>
	svg {
		flex: none;
		display: block;
	}

	polygon,
	circle {
		stroke: #ffffff;
		stroke-width: 2.5;
		stroke-linejoin: round;
		/* The edge lies under the shape, so the shape keeps its size. */
		paint-order: stroke;
	}

	.old circle {
		fill: var(--icon-quiet);
	}
</style>
