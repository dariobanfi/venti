<!--
	What the forecast says about flying, as the design pack writes a status:
	an icon, a word and a colour, never the colour alone. The word is the
	core's: "Flyable", "Marginal", "Not flyable".
-->
<script lang="ts">
	import type { Flyability } from '#lib/core/index.ts';
	import Icon from '#lib/design/Icon.svelte';

	interface Props {
		verdict: Flyability;
		status: string;
	}

	let { verdict, status }: Props = $props();

	const ICONS = { flyable: 'good', marginal: 'warning', notFlyable: 'danger' } as const;
</script>

<span class="status {verdict}">
	<Icon name={ICONS[verdict]} />
	{status}
</span>

<style>
	.status {
		display: inline-flex;
		align-items: center;
		gap: 0.3125rem;
		min-height: 1.75rem;
		padding: 0 0.625rem 0 0.4375rem;
		border-radius: var(--radius-s);
		font-size: var(--subheadline);
		font-weight: 600;
		white-space: nowrap;
		/* The pack's status icons are a little smaller than the text's. */
		--icon-size: 1.0625rem;
	}

	.status :global(svg) {
		width: 1.0625rem;
		height: 1.0625rem;
	}

	.flyable {
		background: var(--color-good-soft);
		color: var(--color-good);
	}

	.marginal {
		background: var(--color-caution-soft);
		color: var(--color-caution);
	}

	.notFlyable {
		background: var(--color-danger-soft);
		color: var(--color-danger);
	}
</style>
