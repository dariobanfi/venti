<!--
	Everything the map can show and do, in one place: its layers as rows
	with a switch, and under a second heading the tools as rows that open
	what the round buttons used to. The two headings are the core's
	(`mapLayersWords`). The map itself keeps four chips and two
	buttons; the rest is here, so that a pilot who wants only the map sees
	only the map.

	The page gives the rows: this component decides nothing about a layer.
-->
<script lang="ts" module>
	import type { IconName } from '#lib/design/Icon.svelte';

	/** A layer of the map: what it is called, whether it is on, and what is said under it. */
	export interface LayerRow {
		key: string;
		name: string;
		icon: IconName;
		on: boolean;
		onchange: (on: boolean) => void;
		/** Lines under the row while the layer is on: what it is, why nothing shows. */
		notes?: readonly string[];
		/** What its data is given under, behind a link of the core's title, while the layer is on. */
	}

	/** A tool of the map: a row that opens something, or leads somewhere. */
	export interface ToolRow {
		key: string;
		name: string;
		icon: IconName;
		href?: string;
		onclick?: () => void;
	}
</script>

<script lang="ts">
	import Icon from '#lib/design/Icon.svelte';

	interface Props {
		layers: readonly LayerRow[];
		tools: readonly ToolRow[];
		/** The core's headings over the two: "On the map", "Lists and plans". */
		layersTitle: string;
		toolsTitle: string;
	}

	let { layers, tools, layersTitle, toolsTitle }: Props = $props();

	const id = $props.id();
</script>

<h3 class="group-title">{layersTitle}</h3>
<div class="group-card" role="group" aria-label={layersTitle}>
	{#each layers as layer (layer.key)}
		{@const notes = layer.on ? (layer.notes ?? []).filter(Boolean) : []}
		<div class="layer">
			<label class="group-row toggle">
				<span class="named"><Icon name={layer.icon} />{layer.name}</span>
				<input
					type="checkbox"
					role="switch"
					aria-describedby={notes.length > 0 ? `${id}-${layer.key}` : undefined}
					checked={layer.on}
					onchange={(event) => layer.onchange(event.currentTarget.checked)}
				/>
			</label>
			{#if notes.length > 0}
				<p class="notes" id="{id}-{layer.key}">
					{#each notes as note (note)}<span>{note}</span>{/each}
				</p>
			{/if}
		</div>
	{/each}
</div>

{#if tools.length > 0}
	<h3 class="group-title tools">{toolsTitle}</h3>
	<div class="group-card" role="group" aria-label={toolsTitle}>
		{#each tools as tool (tool.key)}
			{#if tool.href}
				<a class="group-row tool" href={tool.href}>
					<span class="named"><Icon name={tool.icon} />{tool.name}</span>
				</a>
			{:else}
				<button class="group-row tool" type="button" onclick={tool.onclick}>
					<span class="named"><Icon name={tool.icon} />{tool.name}</span>
				</button>
			{/if}
		{/each}
	</div>
{/if}

<style>
	.named {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		min-width: 0;
	}

	.named :global(svg) {
		flex: none;
		width: 1.25rem;
		height: 1.25rem;
		color: var(--text-secondary);
	}

	.tools {
		margin-top: var(--space-4);
	}

	.tool {
		color: var(--link);
	}

	.tool :global(svg) {
		color: var(--link);
	}

	/* What a layer says of itself, under its row and inside its card. */
	.notes {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		margin: 0;
		padding: 0 var(--space-4) var(--space-3);
		color: var(--text-secondary);
		font-size: var(--footnote);
	}

	/* Compact on a wide screen, as the rest of the map's side. */
	@media (min-width: 56rem) {
		.group-card :global(.group-row) {
			min-height: var(--target);
			padding-block: var(--space-1);
			font-size: var(--subheadline);
		}
	}
</style>
