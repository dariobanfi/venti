<!--
	A pair's level as the design pack writes a status: a word and a colour,
	and a sign where there is something to heed. The word is the core's
	(`FoehnRow.levelLabel`). "Not known" is grey; "No sign" is plain and
	never green, because the pressure alone does not make a day good.
-->
<script lang="ts">
	import type { FoehnLevel } from '#lib/core/index.ts';
	import Icon from '#lib/design/Icon.svelte';

	interface Props {
		level: FoehnLevel;
		label: string;
	}

	let { level, label }: Props = $props();
</script>

<span class="level {level}" data-level={level}>
	{#if level === 'possible'}
		<Icon name="warning" />
	{:else if level === 'likely'}
		<Icon name="danger" />
	{/if}
	{label}
</span>

<style>
	/* Not the app's `.badge`, which is a count on the primary colour. */
	.level {
		display: inline-flex;
		align-items: center;
		gap: 0.3125rem;
		min-height: 1.75rem;
		padding: 0 0.625rem;
		border-radius: var(--radius-s);
		font-size: var(--subheadline);
		font-weight: 600;
		white-space: nowrap;
	}

	.level :global(svg) {
		width: 1.0625rem;
		height: 1.0625rem;
	}

	.unknown {
		background: var(--fill);
		color: var(--text-secondary);
	}

	.none {
		border: 1px solid var(--separator);
		color: var(--text);
	}

	.possible {
		background: var(--color-caution-soft);
		color: var(--color-caution);
	}

	.likely {
		background: var(--color-danger-soft);
		color: var(--color-danger);
	}
</style>
