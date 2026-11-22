<!--
	A card whose one row folds what is under it, closed from the start, as
	the phones have it: "More about the air here" on a station's page, "How
	this is read" on a site's. The row opens and closes the same content; a
	screen reader hears it as a group named by the row's words.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';
	import Icon from './Icon.svelte';

	interface Props {
		/** The row's words, the core's. */
		title: string;
		children: Snippet;
	}

	let { title, children }: Props = $props();
</script>

<details class="card fold-row" aria-label={title}>
	<summary>
		<span>{title}</span>
		<span class="turn" aria-hidden="true"><Icon name="forward" /></span>
	</summary>
	<div class="folded">
		{@render children()}
	</div>
</details>

<style>
	.fold-row {
		background: var(--surface);
		border-radius: var(--radius-card);
		overflow: hidden;
	}

	summary {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-4);
		min-height: var(--target);
		padding: var(--space-2) var(--space-4);
		list-style: none;
		cursor: pointer;
	}

	summary::-webkit-details-marker {
		display: none;
	}

	.turn {
		display: grid;
		flex: none;
		color: var(--icon-quiet);
		font-size: var(--footnote);
		transform: rotate(90deg);
	}

	.fold-row[open] .turn {
		transform: rotate(-90deg);
	}

	summary:focus-visible {
		outline-offset: -0.1875rem;
		border-radius: var(--radius-card);
	}

	@media (hover: hover) {
		summary:hover {
			background: var(--fill);
		}
	}

	.folded {
		padding: 0 var(--space-4) var(--space-4);
	}
</style>
