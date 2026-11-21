<!--
	The card of the map tab's "Forecast" layer: the days the thermal map
	holds as the pack's segmented control, the core's key with swatches in
	the map's colours, the model and its run, and the day's note. While the
	map is asked for, or when there is none, it says so in place of the key,
	with "Try again".

	Where the server has the hours of its days, a row of the layers above
	the days ("Thermals", "Wind", "Gusts", "Cloud", "Rain"), the height of
	the wind beside it, and under the days the line of hours. The key, the
	model and the notes are then the layer's. On "Thermals" the line opens
	at "Day", the day as a whole as before.

	A square on the map cannot be reached with the keys, so the card says
	where the same reading is found for a take-off.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { ThermalMap, ThermalMapDay, ThermalMapHoursView } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import { keyColor, NOT_FLYABLE_ON_MAP, THERMAL_ON_MAP } from '#lib/design/palette.ts';
	import Segmented from '#lib/design/Segmented.svelte';
	import HourStrip from './HourStrip.svelte';
	import { runLine, WORDS } from './thermal-map';

	interface Props {
		map: ThermalMap | undefined;
		day: ThermalMapDay | undefined;
		asking: boolean;
		/** Why there is no map, in words for the pilot. */
		problem: string | undefined;
		/** Whether the day shown has squares in the part of the map in view. */
		inView: boolean;
		/** Shows another day, by its date. */
		onday: (date: string) => void;
		onretry: () => void;
		/** The layer chosen, by the core's name, and the height of the wind. */
		layer?: string;
		heightM?: number;
		/** The hours of the layer chosen, once read, and the hour shown; nothing for the day. */
		view?: ThermalMapHoursView;
		hour?: number;
		/** Whether the hours are being asked for, and why they could not be had. */
		hoursAsking?: boolean;
		hoursProblem?: string;
		onlayer?: (layer: string) => void;
		onheight?: (heightM: number | undefined) => void;
		/** Shows an hour by its number, or the day as a whole. */
		onhour?: (hour: number | 'day') => void;
		onretryhours?: () => void;
		/** What stands under the key: on a wide screen, the square chosen. */
		children?: Snippet;
	}

	let {
		map,
		day,
		asking,
		problem,
		inView,
		onday,
		onretry,
		layer = 'thermals',
		heightM,
		view,
		hour,
		hoursAsking = false,
		hoursProblem,
		onlayer,
		onheight,
		onhour,
		onretryhours,
		children
	}: Props = $props();

	/** The layer chosen, as the map offers it; nothing where the map has no hours. */
	const choice = $derived(map?.layers.find((one) => one.layer === layer) ?? map?.layers[0]);
	/** The line of hours: "Day" first on "Thermals", then each hour. */
	const cells = $derived([
		...(view?.layer === 'thermals' ? [{ short: WORDS.day, label: WORDS.day }] : []),
		...(view?.hours.map((one) => ({ short: String(one.clockHour), label: one.label })) ?? [])
	]);
	const dayCell = $derived(view?.layer === 'thermals' ? 1 : 0);
	/** Whether the key and the run are the hour's rather than the day's. */
	const hourly = $derived(view !== undefined && hour !== undefined && choice !== undefined);

	const text = format();

	const names = $derived(map?.days.map((one) => one.name) ?? []);
	let chosen = $state('');
	$effect(() => {
		chosen = day?.name ?? '';
	});
	$effect(() => {
		const picked = map?.days.find((one) => one.name === chosen);
		if (picked && picked.date !== day?.date) onday(picked.date);
	});

	/** The map's colour of a line of the key. */
	function swatch(step: number | undefined): string {
		return step === undefined ? NOT_FLYABLE_ON_MAP : THERMAL_ON_MAP[step - 1];
	}
</script>

<section class="thermal-map" aria-labelledby="thermal-map-title">
	<h2 id="thermal-map-title" class="visually-hidden">Forecast map</h2>
	{#if map && day}
		{#if map.layers.length > 0}
			<div class="layers">
				<div class="chips" role="group" aria-label="Layer">
					{#each map.layers as one (one.layer)}
						<button
							class="chip"
							type="button"
							aria-pressed={one.layer === choice?.layer}
							onclick={() => onlayer?.(one.layer)}
						>
							{one.name}
						</button>
					{/each}
				</div>
				{#if choice?.hasLevels && map.levels.length > 0}
					<label class="level">
						<span class="visually-hidden">Height</span>
						<select
							value={String(heightM ?? '')}
							onchange={(event) => {
								const picked = event.currentTarget.value;
								onheight?.(picked === '' ? undefined : Number(picked));
							}}
						>
							{#each map.levels as level (level.name)}
								<option value={String(level.heightM ?? '')}>{level.name}</option>
							{/each}
						</select>
					</label>
				{/if}
			</div>
		{/if}
		{#if names.length > 1}
			<Segmented label="Day" options={names} bind:value={chosen} />
		{/if}
		{#if map.layers.length > 0}
			{#if view}
				<HourStrip
					label="Hour"
					{cells}
					value={hour === undefined ? 0 : hour + dayCell}
					onchange={(index) => onhour?.(index < dayCell ? 'day' : index - dayCell)}
				/>
				{#if hour !== undefined && view.hours[hour]}
					<p class="title">{view.title}, {view.hours[hour].label}</p>
				{/if}
			{:else if hoursAsking}
				<p class="waiting" role="status">{WORDS.askingHours}</p>
			{:else if hoursProblem}
				<p class="caption" role="status">
					{hoursProblem}
					<button class="again" type="button" onclick={onretryhours}>Try again</button>
				</p>
			{/if}
		{/if}
		<h3 class="label" id="thermal-map-key">Key</h3>
		<ul class="key" aria-labelledby="thermal-map-key">
			{#if hourly && choice}
				{#each choice.legend as entry (entry.step)}
					<li>
						<span class="swatch" style:background={keyColor(entry)} aria-hidden="true"></span>
						{entry.words}
					</li>
				{/each}
			{:else}
				{#each map.legend as entry (entry.words)}
					<li>
						<span class="swatch" style:background={swatch(entry.scaleStep)} aria-hidden="true"
						></span>
						{entry.words}
					</li>
				{/each}
			{/if}
		</ul>
		{#if hourly && view}
			<p class="caption numeric">{runLine(view, text)}</p>
			{#if choice?.note}<p class="caption">{choice.note}</p>{/if}
			{#if day.note}<p class="caption">{day.note}</p>{/if}
			{#if !view.whole && view.note}<p class="caption">{view.note}</p>{/if}
		{:else}
			<p class="caption numeric">{runLine(day, text)}</p>
			{#if day.note}
				<p class="caption">{day.note}</p>
			{/if}
		{/if}
		{#if !inView}
			<p class="caption">{WORDS.noneHere}</p>
		{/if}
		{#if problem && !asking}
			<p class="caption" role="status">
				{problem}
				<button class="again" type="button" onclick={onretry}>Try again</button>
			</p>
		{/if}
		<!-- A square cannot be reached with the keys; a take-off can. -->
		<p class="caption">
			The same reading for each take-off near you is in the list of <a href="/today/">Today</a>.
		</p>
		{@render children?.()}
	{:else if asking}
		<p class="waiting" role="status">{WORDS.asking}</p>
	{:else}
		<div role="status">
			<p>{problem ?? WORDS.noDays}</p>
			<button class="again" type="button" onclick={onretry}>Try again</button>
		</div>
	{/if}
</section>

<style>
	.thermal-map {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.layers {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		min-width: 0;
	}

	/* The pack's chips, one of them on; the row scrolls sideways where it does not fit. */
	.chips {
		display: flex;
		gap: var(--space-2);
		min-width: 0;
		overflow-x: auto;
		scrollbar-width: none;
	}

	.chip {
		flex: none;
		min-height: 2.25rem;
		padding: 0 var(--space-3);
		border-radius: var(--radius-pill);
		background: var(--surface);
		box-shadow: inset 0 0 0 1px var(--separator);
		color: var(--text);
		font-size: var(--subheadline);
		font-weight: 600;
		white-space: nowrap;
	}

	.chip[aria-pressed='true'] {
		background: var(--accent);
		color: var(--on-accent);
		box-shadow: none;
	}

	.level select {
		min-height: 2.25rem;
		padding: 0 var(--space-2);
		border: 0;
		border-radius: var(--radius-control);
		background: var(--fill);
		color: var(--text);
		font: inherit;
		font-size: var(--subheadline);
	}

	.title {
		margin: 0;
		font-size: var(--subheadline);
		font-weight: 600;
	}

	.label {
		margin: 0;
		color: var(--text-secondary);
		font-size: var(--caption);
		font-weight: 600;
	}

	.key {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(8rem, 1fr));
		gap: var(--space-1) var(--space-3);
		margin: 0;
		padding: 0;
		list-style: none;
		font-size: var(--subheadline);
	}

	.key li {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	/* The square as the map draws it, with an edge, since the palest steps
	   lie close to the card's surface. */
	.swatch {
		flex: none;
		width: 0.875rem;
		height: 0.875rem;
		border: 1px solid var(--color-border-strong);
	}

	.caption {
		margin: 0;
		color: var(--text-secondary);
		font-size: var(--caption);
	}

	.waiting {
		margin: 0;
		color: var(--text-secondary);
		font-size: var(--subheadline);
	}

	p {
		margin: 0;
	}

	.again {
		margin-top: var(--space-1);
		color: var(--link);
		font-weight: 600;
	}

	.caption .again {
		margin: 0 0 0 var(--space-1);
		font-size: inherit;
	}
</style>
