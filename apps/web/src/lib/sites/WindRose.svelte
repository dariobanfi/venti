<!--
	The wind directions a site works in, as wedges on a small compass: north
	at the top, each wedge where the wind may come from. The words beside it
	say the same, so a screen reader passes over it.
-->
<script lang="ts">
	import type { WindSpan } from '#lib/core/index.ts';
	import { wedges } from './rose';

	let { spans }: { spans: readonly WindSpan[] } = $props();

	const SIDE = 30;
	const shapes = $derived(wedges(spans, SIDE / 2, SIDE / 2 - 1));
</script>

<svg
	viewBox="0 0 {SIDE} {SIDE}"
	width="1.875rem"
	height="1.875rem"
	aria-hidden="true"
	focusable="false"
>
	{#each shapes as shape (shape)}
		<path d={shape} />
	{/each}
	<circle cx={SIDE / 2} cy={SIDE / 2} r={SIDE / 2 - 1} />
</svg>

<style>
	svg {
		flex: none;
		display: block;
	}

	circle {
		fill: none;
		stroke: var(--text-secondary);
		stroke-width: 1;
	}

	path {
		fill: var(--text);
		opacity: 0.75;
	}
</style>
