<!--
	The heading of a column that sorts, in a table of a wide screen: a click
	orders the rows by it, a second turns the order round. The arrow is the
	pack's chevron, pointing down for the largest first; `aria-sort` says the
	same to a screen reader.
-->
<script lang="ts" generics="Key extends string">
	import type { Snippet } from 'svelte';
	import Icon from './Icon.svelte';
	import { ariaSort, type Sort } from './sort';

	interface Props {
		key: Key;
		sort: Sort<Key> | undefined;
		numeric?: boolean;
		onsort: (key: Key) => void;
		children: Snippet;
	}

	let { key, sort, numeric = false, onsort, children }: Props = $props();
</script>

<th class:numeric aria-sort={ariaSort(sort, key)} scope="col">
	<button type="button" onclick={() => onsort(key)}>
		{@render children()}
		{#if sort?.key === key}
			<span class="arrow" class:up={!sort.descending} aria-hidden="true">
				<Icon name="forward" />
			</span>
		{/if}
	</button>
</th>

<style>
	th {
		height: 2rem;
		padding: 0 var(--space-2);
		text-align: left;
		white-space: nowrap;
		font-size: var(--subheadline);
		font-weight: 600;
		color: var(--text-secondary);
		box-shadow: inset 0 -1px 0 var(--color-border);
	}

	.numeric {
		text-align: right;
	}

	button {
		display: inline-flex;
		align-items: center;
		gap: 0.125rem;
		font: inherit;
		color: inherit;
	}

	.numeric button {
		flex-direction: row-reverse;
	}

	.arrow {
		display: inline-flex;
		transform: rotate(90deg);
	}

	.arrow.up {
		transform: rotate(-90deg);
	}

	.arrow :global(svg) {
		width: 0.875rem;
		height: 0.875rem;
	}
</style>
