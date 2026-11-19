<!--
	The map's sign for a flying site, drawn for a list or a heading: the same
	disc with the same sign, dark for a take-off and light for a landing. It
	looks the same in both appearances, as the map does. It only repeats what
	the text beside it says, so a screen reader passes over it.
-->
<script lang="ts">
	import type { SiteKind } from '#lib/core/index.ts';
	import {
		colours,
		DISC_RADIUS,
		EDGE_WIDTH,
		SIDE,
		SIGN_BOX,
		SIGN_LINE,
		SIGN_SIDE,
		SIGNS
	} from './symbols';

	let { kind }: { kind: SiteKind } = $props();

	const look = $derived(colours(kind));
	/** Puts the sign's box in the middle of the disc, at the sign's size. */
	const placed = [
		`translate(${SIDE / 2} ${SIDE / 2})`,
		`scale(${SIGN_SIDE / SIGN_BOX})`,
		`translate(${-SIGN_BOX / 2} ${-SIGN_BOX / 2})`
	].join(' ');
</script>

<svg
	viewBox="0 0 {SIDE} {SIDE}"
	width="1.75rem"
	height="1.75rem"
	aria-hidden="true"
	focusable="false"
>
	<circle
		cx={SIDE / 2}
		cy={SIDE / 2}
		r={DISC_RADIUS}
		fill={look.ground}
		stroke={look.sign}
		stroke-width={EDGE_WIDTH}
	/>
	<path
		d={SIGNS[kind]}
		transform={placed}
		fill="none"
		stroke={look.sign}
		stroke-width={SIGN_LINE}
		stroke-linecap="round"
		stroke-linejoin="round"
	/>
</svg>

<style>
	svg {
		flex: none;
		display: block;
	}
</style>
