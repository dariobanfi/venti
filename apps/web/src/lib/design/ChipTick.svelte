<!--
	The tick an "on" chip carries before its word, the pack's `check`, in
	every theme: on and off never rest on colour alone (design/DESIGN.md,
	"Chip"; the owner, 2026-11-23). It says nothing to a screen reader: the
	chip's `aria-pressed` does.
-->
<script lang="ts">
	import Icon from './Icon.svelte';

	interface Props {
		/** Whether the chip is on. */
		on: boolean;
	}

	let { on }: Props = $props();
</script>

{#if on}<span class="tick"><Icon name="check" /></span>{/if}

<style>
	.tick {
		display: inline-flex;
		vertical-align: -0.1875rem;
		margin-inline-end: var(--space-1);
	}

	/* The pack's 16 px, growing with the reader's text. */
	.tick :global(svg) {
		width: 1rem;
		height: 1rem;
	}
</style>
