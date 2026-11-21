<!--
	One square of the forecast map in the hour shown, chosen on the map: the
	square's days side by side, the hour and the layer in the core's words
	(`title`, `detail`), the status on "Thermals" and the arrow in its band
	on "Wind" and "Gusts", the height the model's ground lies at, the way to
	the forecast for the place, the notes and the credits. A click where no
	square is drawn opens it too: "Dry" or "Mostly clear" is a reading.
-->
<script lang="ts">
	import type { ThermalMapHourSquare } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import StatusChip from '#lib/today/StatusChip.svelte';
	import WindGlyph from '#lib/wind/WindGlyph.svelte';
	import SquareDays from './SquareDays.svelte';
	import type { ThermalMapModel } from './thermal-map.svelte';
	import { judgedAt, runLine } from './thermal-map';

	interface Props {
		thermals: ThermalMapModel;
		/** The place clicked, and the number of its square. */
		place: { lat: number; lon: number };
		cell: number;
		/** Opens the forecast for the middle of the square. */
		onforecast: (square: { south: number; west: number; north: number; east: number }) => void;
	}

	let { thermals, place, cell, onforecast }: Props = $props();

	const text = format();

	let square = $state.raw<ThermalMapHourSquare>();

	// The square's words, again whenever the hour, the layer or the day changes.
	$effect(() => {
		void thermals.view;
		void thermals.hour;
		const asking = cell;
		void thermals.square(asking).then((read) => {
			if (asking === cell) square = read;
		});
	});

	const choice = $derived(thermals.map?.layers.find((one) => one.layer === thermals.view?.layer));
</script>

<div class="square">
	<SquareDays {thermals} {place} />
	{#if square}
		<div class="reading">
			{#if square.wind}
				<WindGlyph mark={square.wind} />
			{/if}
			<div>
				<p class="title">{square.title}</p>
				{#if square.detail}<p class="secondary">{square.detail}</p>{/if}
			</div>
		</div>
		{#if square.thermals}
			<p class="status">
				<StatusChip verdict={square.thermals.verdict} status={square.thermals.status} />
				{#if square.thermals.why}<span class="secondary">{square.thermals.why}</span>{/if}
			</p>
			{#if square.thermals.thermalLine}
				<p class="secondary">{square.thermals.thermalLine}</p>
			{/if}
		{/if}
		{#if square.groundM !== undefined}
			<p class="secondary">{judgedAt(square.groundM, text)}</p>
		{/if}
		<button class="forecast" type="button" onclick={() => onforecast(square!)}
			>Forecast for this place</button
		>
	{/if}
	{#if choice?.note}<p class="caption">{choice.note}</p>{/if}
	{#if thermals.day?.note}<p class="caption">{thermals.day.note}</p>{/if}
	{#if thermals.view}
		{#if !thermals.view.whole && thermals.view.note}
			<p class="caption">{thermals.view.note}</p>
		{/if}
		<p class="caption numeric">{runLine(thermals.view, text)}</p>
		{#if thermals.view.credits.length > 0}
			<p class="caption">
				{#each thermals.view.credits as credit, index (credit.source)}
					{#if index > 0}<br />{/if}
					{#if credit.url}<a href={credit.url} rel="noopener noreferrer" target="_blank">
							{credit.text || credit.provider}
						</a>{:else}{credit.text || credit.provider}{/if}
				{/each}
			</p>
		{/if}
	{/if}
</div>

<style>
	.square {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	p {
		margin: 0;
	}

	.reading {
		display: flex;
		align-items: center;
		gap: var(--space-3);
	}

	.title {
		font-weight: 600;
	}

	.status {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
	}

	.forecast {
		align-self: flex-start;
		min-height: var(--target);
		color: var(--link);
		font-size: var(--subheadline);
		font-weight: 600;
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
