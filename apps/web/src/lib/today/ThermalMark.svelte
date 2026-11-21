<!--
	A round mark in the colour of a forecast climb, from the pack's thermal
	scale. The kind of day or the number stands beside it in the text's own
	colour, as the pack asks of every scale: the colour alone says nothing,
	so the mark is hidden from a screen reader. Its edge is the strong
	border, since the scale's palest step lies close to the surface.
-->
<script lang="ts">
	import { thermalStep } from '#lib/design/palette.ts';

	interface Props {
		climbMs: number;
	}

	let { climbMs }: Props = $props();
</script>

<span class="mark step-{thermalStep(climbMs)}" aria-hidden="true"></span>

<style>
	.mark {
		display: inline-block;
		flex: none;
		width: 0.75rem;
		height: 0.75rem;
		border: 1px solid var(--color-border-strong);
		border-radius: 50%;
	}

	.step-1 {
		background: var(--thermal-1);
	}

	.step-2 {
		background: var(--thermal-2);
	}

	.step-3 {
		background: var(--thermal-3);
	}

	.step-4 {
		background: var(--thermal-4);
	}

	.step-5 {
		background: var(--thermal-5);
	}

	.step-6 {
		background: var(--thermal-6);
	}
</style>
