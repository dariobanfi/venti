<!--
	The faces of a few pilots in a row, beside the core's line that counts
	them: who follows a pilot, who cheered a flight. The first page the core
	read, newest first, at most a handful; the line says how many there are.
	The faces carry no words of their own, as `PilotFace` does not: the line
	beside them is what a screen reader hears.
-->
<script lang="ts">
	import PilotFace from './PilotFace.svelte';

	interface Props {
		pilots: { handle: string; name: string; picturePath?: string }[];
		/** How many faces at most. */
		most?: number;
	}

	let { pilots, most = 6 }: Props = $props();
	const shown = $derived(pilots.slice(0, most));
</script>

{#if shown.length > 0}
	<span class="faces">
		{#each shown as pilot (pilot.handle)}
			<PilotFace picture={pilot.picturePath} name={pilot.name} />
		{/each}
	</span>
{/if}

<style>
	.faces {
		display: inline-flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-1);
	}

	.faces :global(.face) {
		width: 1.5rem;
		height: 1.5rem;
	}
</style>
