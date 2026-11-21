<!--
	The card of the map tab's "Forecast" layer: the days the thermal map
	holds as the pack's segmented control, the core's key with swatches in
	the map's colours, the model and its run, and the day's note. While the
	map is asked for, or when there is none, it says so in place of the key,
	with "Try again".

	A square on the map cannot be reached with the keys, so the card says
	where the same reading is found for a take-off.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { ThermalMap, ThermalMapDay } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import { NOT_FLYABLE_ON_MAP, THERMAL_ON_MAP } from '#lib/design/palette.ts';
	import Segmented from '#lib/design/Segmented.svelte';
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
		/** What stands under the key: on a wide screen, the square chosen. */
		children?: Snippet;
	}

	let { map, day, asking, problem, inView, onday, onretry, children }: Props = $props();

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
		{#if names.length > 1}
			<Segmented label="Day" options={names} bind:value={chosen} />
		{/if}
		<h3 class="label" id="thermal-map-key">Key</h3>
		<ul class="key" aria-labelledby="thermal-map-key">
			{#each map.legend as entry (entry.words)}
				<li>
					<span class="swatch" style:background={swatch(entry.scaleStep)} aria-hidden="true"></span>
					{entry.words}
				</li>
			{/each}
		</ul>
		<p class="caption numeric">{runLine(map, text)}</p>
		{#if day.note}
			<p class="caption">{day.note}</p>
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
