<!--
	The wind near by when it is much stronger than at the take-off, as a note
	in the caution manner: the pack's caution sign and colour beside the
	core's line, as the rain radar marks old rain. It is no verdict, so it
	has no ground and no pill, and stands under the day's own reason and
	never in the status's place. Nothing where the core's line is empty.
-->
<script lang="ts">
	import Icon from '#lib/design/Icon.svelte';

	interface Props {
		/** The core's line: `OutlookDay.nearby`, or an hour's `nearby.line`. */
		words: string;
		/** In the caption's size, for a table's row and an hour's detail. */
		small?: boolean;
	}

	let { words, small = false }: Props = $props();
</script>

{#if words}
	<span class="nearby" class:small>
		<Icon name="warning" />
		<span data-testid="nearby-line">{words}</span>
	</span>
{/if}

<style>
	.nearby {
		display: flex;
		align-items: flex-start;
		gap: var(--space-2);
		color: var(--color-caution);
		font-size: var(--subheadline);
		font-weight: 500;
		text-align: left;
		white-space: normal;
		overflow-wrap: anywhere;
	}

	.small {
		gap: var(--space-1);
		font-size: var(--caption);
	}

	/* The sign at the height of the line's first row. */
	.nearby :global(svg) {
		width: 1.25em;
		height: 1.25em;
	}
</style>
