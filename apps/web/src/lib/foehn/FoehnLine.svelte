<!--
	Foehn on Today: the core's one line, there only when a pair of stations
	near the take-offs is at "possible" or "likely", in the manner of the
	wind near by: the pack's caution sign beside the words, in the caution's
	colour for "possible" and the danger's for "likely", with no ground. It
	changes no take-off's status. A tap opens the foehn view.
-->
<script lang="ts">
	import type { FoehnLine } from '#lib/core/index.ts';
	import Icon from '#lib/design/Icon.svelte';

	interface Props {
		line: FoehnLine;
		onopen: () => void;
	}

	let { line, onopen }: Props = $props();
</script>

<button class="foehn {line.level}" type="button" data-testid="foehn-line" onclick={onopen}>
	<Icon name="warning" />
	<span>{line.text}</span>
	<span class="more"><Icon name="forward" /></span>
</button>

<style>
	.foehn {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		width: 100%;
		min-height: var(--size-touch);
		margin-bottom: var(--space-4);
		padding: var(--space-2) var(--space-4);
		border-radius: var(--radius-control);
		font-size: var(--subheadline);
		font-weight: 500;
		text-align: left;
		overflow-wrap: anywhere;
	}

	.foehn > span:not(.more) {
		flex: 1;
		min-width: 0;
	}

	.possible {
		color: var(--color-caution);
	}

	.likely {
		color: var(--color-danger);
	}

	.foehn :global(svg) {
		width: 1.25em;
		height: 1.25em;
	}

	@media (hover: hover) {
		.foehn:hover {
			background: var(--fill);
		}
	}
</style>
