<!--
	One square of the thermal map, chosen on the map: the day's name, the
	status as icon, word and colour, the kind of day with its mark in the
	square's colour on the map and the thermals in a line, the height it was
	judged at, the day's and the map's notes and the credits. The words are
	the core's.
-->
<script lang="ts">
	import type { ThermalMap, ThermalMapCell, ThermalMapDay } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import { THERMAL_ON_MAP } from '#lib/design/palette.ts';
	import StatusChip from '#lib/today/StatusChip.svelte';
	import ThermalLine from '#lib/today/ThermalLine.svelte';
	import { judgedAt, runLine } from './thermal-map';

	interface Props {
		map: ThermalMap;
		day: ThermalMapDay;
		cell: ThermalMapCell;
		/** Whether the day's name stands at the head: not where the panel's title says it. */
		named?: boolean;
		/** Whether the model's run is said: not where the key above says it. */
		withRun?: boolean;
		/** Opens the forecast for the middle of the square, as a site's page does. */
		onforecast?: () => void;
	}

	let { map, day, cell, named = true, withRun = true, onforecast }: Props = $props();

	const text = format();
</script>

<div class="square">
	{#if named}
		<h3>{day.name}</h3>
	{/if}
	<!-- The status, and beside it what speaks against the day where something does. -->
	<p class="status">
		<StatusChip verdict={cell.verdict} status={cell.status} />
		{#if cell.why}<span class="secondary">{cell.why}</span>{/if}
	</p>
	{#if cell.thermalWords}
		<ThermalLine
			day={cell}
			color={cell.scaleStep === undefined ? undefined : THERMAL_ON_MAP[cell.scaleStep - 1]}
		/>
	{/if}
	{#if cell.groundM !== undefined}
		<p class="secondary">{judgedAt(cell.groundM, text)}</p>
	{/if}
	{#if onforecast}
		<!-- The hours behind the day's verdict, with the two rows of thermals. -->
		<button class="forecast" type="button" onclick={onforecast}>Forecast for this place</button>
	{/if}
	{#if day.note}
		<p class="caption">{day.note}</p>
	{/if}
	<p class="caption">{map.note}</p>
	{#if withRun}
		<p class="caption numeric">{runLine(map, text)}</p>
	{/if}
	{#if map.credits.length > 0}
		<p class="caption">
			{#each map.credits as credit, index (credit.source)}
				{#if index > 0}<br />{/if}
				{#if credit.url}<a href={credit.url} rel="noopener noreferrer" target="_blank">
						{credit.text || credit.provider}
					</a>{:else}{credit.text || credit.provider}{/if}
			{/each}
		</p>
	{/if}
</div>

<style>
	.square {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	h3 {
		margin: 0;
		font-size: var(--subheadline);
	}

	p {
		margin: 0;
	}

	.forecast {
		align-self: flex-start;
		min-height: var(--target);
		color: var(--link);
		font-size: var(--subheadline);
		font-weight: 600;
	}

	.status {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
	}

	.secondary {
		color: var(--text-secondary);
		font-size: var(--subheadline);
	}

	.caption {
		color: var(--text-secondary);
		font-size: var(--caption);
	}
</style>
