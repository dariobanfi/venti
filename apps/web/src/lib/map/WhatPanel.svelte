<!--
	The one panel over the map for what it shows (the owner, 2026-11-22:
	"we don't need all of this text, just a legend"): one line that says
	what is shown and when ("Rain, 21:25, measured"; "Wind at the ground,
	15:00, forecast, ICON-D2"), the one line of time from what was measured
	to what is forecast, and the key as a short strip, or with the thermals
	by region the five levels with their names. The wind's gusts are a small
	control of the wind's forecast; its height is a pill at the map's side
	(`HeightChoice.svelte`). Whatever
	explains (the whole key, what the radar does not see, the model's run,
	the notes of a day) stands behind the "i", folded until the pilot asks;
	the credits are the map's, behind its own "i".

	What the pilot has to be told about the data as it stands is never
	folded away: old rain that is not the rain of now, a part of the map
	with nothing forecast, and what could not be had, with "Try again".
-->
<script lang="ts">
	import { tick } from 'svelte';
	import { MediaQuery } from 'svelte/reactivity';
	import { getApp } from '#lib/app/app.svelte.ts';
	import ChipTick from '#lib/design/ChipTick.svelte';
	import Icon from '#lib/design/Icon.svelte';
	import {
		keyColor,
		LEVEL_ON_MAP,
		NOT_FLYABLE_ON_MAP,
		THERMAL_ON_MAP
	} from '#lib/design/palette.ts';
	import type { RegionLevelKey } from '#lib/core/index.ts';
	import { fitColour } from '#lib/sites/fans.ts';
	import { wedges } from '#lib/sites/rose.ts';
	import { format } from '#lib/design/format.ts';
	import type { ThermalMapModel } from '#lib/forecast/thermal-map.svelte.ts';
	import { runLine, WORDS as FORECAST_WORDS } from '#lib/forecast/thermal-map.ts';
	import type { RadarModel } from '#lib/radar/radar.svelte.ts';
	import { UNSEEN_ON_MAP, WORDS as RADAR_WORDS } from '#lib/radar/radar.ts';
	import type { What } from './layers';
	import TimeLine from './TimeLine.svelte';
	import { type Cell, shortWords, withoutCaveats, WORDS } from './timeline';

	const app = getApp();
	const text = format();

	interface Props {
		what: What;
		/** What it is called: "Rain". */
		name: string;
		cells: readonly Cell[];
		/** The cell the line stands on. */
		value: number;
		onchoose: (index: number) => void;
		/** Whether the line stands on what was measured. */
		measured: boolean;
		radar?: RadarModel;
		thermals?: ThermalMapModel;
		/** The key of the forecast by region, the best first, while its regions are drawn. */
		regionLevels?: readonly RegionLevelKey[];
		/** The wind's gusts in place of its mean, at the ground. */
		gusts: boolean;
		ongusts: (on: boolean) => void;
		/** Whether the forecast shown has squares in the part of the map in view. */
		inView: boolean;
		/** Whether the take-offs' fans are coloured by the wind shown: their key is then here too. */
		fans?: boolean;
	}

	let {
		what,
		name,
		cells,
		value,
		onchoose,
		measured,
		radar,
		thermals,
		regionLevels,
		gusts,
		ongusts,
		inView,
		fans = false
	}: Props = $props();

	/** The fan the key draws: a quarter of the compass to the north. */
	const FAN = wedges([{ fromDeg: 315, toDeg: 45 }], 8, 8)[0];

	/** The key of the take-offs' fans, while they are coloured by the wind. */
	const fanKey = $derived(
		fans
			? [
					{ fit: 'fits' as const, words: 'Take-off fits' },
					{ fit: 'misfits' as const, words: 'Does not fit' }
				]
			: []
	);

	/** Nothing moves by itself for a pilot who asked the system for less motion. */
	const still = new MediaQuery('prefers-reduced-motion: reduce', false);

	const map = $derived(thermals?.map);
	const day = $derived(thermals?.day);
	const view = $derived(thermals?.view);
	const hour = $derived(thermals?.hour);
	/** The layer of the forecast shown, as the map offers it. */
	const choice = $derived(map?.layers.find((one) => one.layer === thermals?.layer));
	/** Whether the first day is the one shown: its hours need no day's name. */
	const firstDay = $derived(Boolean(day && map?.days[0]?.date === day.date));

	/** What is shown and when, in one line. */
	const line = $derived.by(() => {
		if (measured && what === 'rain') {
			const moment = radar?.shownMoment;
			if (moment) return `${name}, ${moment.clock}, ${WORDS.measured}`;
			return radar?.pictureAsking ? RADAR_WORDS.askingPicture : '';
		}
		if (measured) return `${name}, ${WORDS.now.toLowerCase()}, ${WORDS.measured}`;
		if (view && hour !== undefined && view.hours[hour] && day) {
			const at = view.hours[hour].label;
			const when = firstDay ? at : `${day.name} ${at}`;
			return `${view.title}, ${when}, ${WORDS.forecast}, ${view.model}`;
		}
		if (day && (what === 'thermals' || !map?.layers.length)) {
			return `${name}, ${day.name}, ${WORDS.forecast}, ${day.model}`;
		}
		return thermals?.hoursAsking ? FORECAST_WORDS.askingHours : '';
	});

	/** A line of the key: its colour and its words. */
	interface KeyLine {
		colour: string;
		words: string;
	}

	/**
	 * The key of what is shown, the least first, and the veil apart: the
	 * radar's steps and what it did not see; the hour's layer; the kinds of
	 * day and the day that is not flyable. The wind stations need none:
	 * each says its numbers.
	 */
	const key = $derived.by((): { steps: KeyLine[]; veil?: KeyLine } => {
		if (measured && what === 'rain' && radar?.view) {
			return {
				steps: radar.view.key.toReversed().map((one) => ({
					colour: keyColor(one),
					words: one.words
				})),
				veil: { colour: UNSEEN_ON_MAP, words: radar.view.unseenWords }
			};
		}
		if (measured) return { steps: [] };
		if (choice && hour !== undefined) {
			const lines = choice.legend.toReversed();
			const veil = lines.find((one) => one.scale === 'veil');
			return {
				steps: lines
					.filter((one) => one.scale !== 'veil')
					.map((one) => ({ colour: keyColor(one), words: one.words })),
				veil: veil && { colour: NOT_FLYABLE_ON_MAP, words: veil.words }
			};
		}
		if (map && what === 'thermals') {
			const lines = map.legend.toReversed();
			const veil = lines.find((one) => one.scaleStep === undefined);
			return {
				steps: lines
					.filter((one) => one.scaleStep !== undefined)
					.map((one) => ({ colour: THERMAL_ON_MAP[(one.scaleStep ?? 1) - 1], words: one.words })),
				veil: veil && { colour: NOT_FLYABLE_ON_MAP, words: veil.words }
			};
		}
		return { steps: [] };
	});
	/** The regions' five levels, the worst first as the key's steps are: shown whole, each with its name. */
	const levels = $derived(
		!measured && what === 'thermals'
			? (regionLevels ?? []).toReversed().map((one) => ({
					colour: LEVEL_ON_MAP[one.level].color,
					name: one.name,
					words: `${one.name}: ${one.words}`
				}))
			: []
	);
	const keyLines = $derived([
		...levels,
		...key.steps,
		...(key.veil ? [key.veil] : []),
		...fanKey.map((one) => ({ colour: fitColour(one.fit), words: one.words }))
	]);

	/** What stands behind the "i": the notes of what is shown, without the project's own caveats. */
	const notes = $derived.by((): string[] => {
		if (measured && what === 'rain' && radar?.view) {
			return [
				radar.shownMoment?.label ?? '',
				...radar.view.layers.map((layer) => layer.what),
				withoutCaveats(radar.view.note)
			];
		}
		if (measured) return [];
		if (view && hour !== undefined && day) {
			return [runLine(view, text), choice?.note ?? '', day.note, view.whole ? '' : view.note];
		}
		if (day) return [runLine(day, text), day.note];
		return [];
	});

	/** The core's name of the gusts' layer, where the map has it. */
	const gustsName = $derived(map?.layers.find((one) => one.layer === 'gusts')?.name);

	let open = $state(false);
	let folded = $state<HTMLDivElement>();
	/** Unfolds what stands behind the "i" and brings it into view, or folds it away. */
	async function fold() {
		open = !open;
		if (!open) return;
		await tick();
		folded?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
	}

	const playable = $derived(
		measured && what === 'rain' && (radar?.moments.length ?? 0) > 1 && !still.current
	);
</script>

<section class="what" aria-labelledby="what-title">
	<h2 id="what-title" class="visually-hidden">{name}</h2>
	<div class="head">
		<p class="title numeric">{line}</p>
		{#if playable && radar}
			<button
				class="round"
				type="button"
				aria-label={radar.playing ? 'Stop' : 'Play'}
				title={radar.playing ? 'Stop' : 'Play'}
				onclick={() => (radar.playing ? radar.stop() : void radar.play())}
			>
				<Icon name={radar.playing ? 'pause' : 'play'} />
			</button>
		{/if}
		<button
			class="round"
			type="button"
			aria-label="Key and notes"
			title="Key and notes"
			aria-expanded={open}
			onclick={fold}
		>
			<Icon name="info" />
		</button>
	</div>

	{#if cells.length > 0}
		<TimeLine label="Time" {cells} {value} onchange={onchoose} />
	{/if}

	{#if what === 'wind' && !measured && gustsName}
		<div class="controls">
			<button
				class="toggle-chip"
				type="button"
				aria-pressed={gusts}
				onclick={() => ongusts(!gusts)}
			>
				<ChipTick on={gusts} />
				{gustsName}
			</button>
		</div>
	{/if}

	{#if levels.length > 0}
		<!-- The regions' levels, worst to best: each colour with its name. -->
		<div class="levels" aria-hidden="true">
			{#each levels as one (one.name)}
				<span><i style:background={one.colour}></i>{one.name}</span>
			{/each}
		</div>
	{:else if key.steps.length > 0}
		<div class="key" aria-hidden="true">
			<span class="scale">
				<span class="bars">
					{#each key.steps as step (step.words)}
						<i style:background={step.colour} title={step.words}></i>
					{/each}
				</span>
				<span class="ends">
					<span>{shortWords(key.steps[0].words)}</span>
					{#if key.steps.length > 1}<span>{shortWords(key.steps.at(-1)!.words)}</span>{/if}
				</span>
			</span>
			{#if key.veil}
				<span class="veil">
					<i style:background={key.veil.colour}></i>{shortWords(key.veil.words)}
				</span>
			{/if}
		</div>
	{/if}

	<!-- The take-offs' fans in the wind shown: a small fan of each colour and its words. -->
	{#if fanKey.length > 0}
		<div class="fans" aria-hidden="true">
			{#each fanKey as one (one.fit)}
				<span>
					<svg viewBox="0 0 16 16" width="16" height="16">
						<path d={FAN} fill={fitColour(one.fit)} fill-opacity="0.55" />
					</svg>
					{one.words}
				</span>
			{/each}
		</div>
	{/if}

	<!-- What the pilot has to be told about the data as it stands: never folded away. -->
	{#if measured && what === 'rain' && radar?.caution}
		<p class="stale" role="status"><Icon name="warning" />{radar.caution}</p>
	{/if}
	{#if what === 'rain' && radar?.pictureProblem && measured && !radar.pictureAsking}
		<p class="caption" role="status">
			{radar.pictureProblem}
			<button class="again" type="button" onclick={() => void radar.showMoment()}
				>{app.said?.common.tryAgain}</button
			>
		</p>
	{/if}
	{#if what === 'rain' && radar?.problem && !radar.asking}
		<p class="caption" role="status">
			{radar.problem}
			<button class="again" type="button" onclick={() => void radar.load()}
				>{app.said?.common.tryAgain}</button
			>
		</p>
	{/if}
	{#if !measured && thermals}
		{#if thermals.problem && !thermals.asking}
			<p class="caption" role="status">
				{thermals.problem}
				<button class="again" type="button" onclick={() => void thermals.load()}
					>{app.said?.common.tryAgain}</button
				>
			</p>
		{:else if thermals.hoursProblem && !thermals.hoursAsking}
			<p class="caption" role="status">
				{thermals.hoursProblem}
				<button class="again" type="button" onclick={() => void thermals.loadHours()}
					>{app.said?.common.tryAgain}</button
				>
			</p>
		{:else if !map && thermals.asking}
			<p class="caption" role="status">{FORECAST_WORDS.asking}</p>
		{:else if !inView}
			<p class="caption">{FORECAST_WORDS.noneHere}</p>
		{/if}
	{/if}
	{#if cells.length === 0 && what === 'rain' && radar?.asking}
		<p class="caption" role="status">{RADAR_WORDS.asking}</p>
	{/if}

	<div class="folded" bind:this={folded}>
		{#if open}
			{#if keyLines.length > 0}
				<h3 class="label" id="what-key">Key</h3>
				<ul class="lines" aria-labelledby="what-key">
					{#each keyLines as one (one.words)}
						<li><span class="swatch" style:background={one.colour}></span>{one.words}</li>
					{/each}
				</ul>
			{/if}
			{#each notes.filter(Boolean) as note (note)}
				<p class="caption">{note}</p>
			{/each}
		{:else if keyLines.length > 0}
			<ul class="visually-hidden" aria-label="Key">
				{#each keyLines as one (one.words)}<li>{one.words}</li>{/each}
			</ul>
		{/if}
	</div>
</section>

<style>
	.what {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		min-width: 0;
	}

	p {
		margin: 0;
	}

	/* The line in words on the left, the round controls on the right. */
	.head {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		min-width: 0;
	}

	.title {
		flex: 1;
		min-width: 0;
		font-size: var(--subheadline);
		font-weight: 600;
	}

	.round {
		flex: none;
		display: grid;
		place-items: center;
		width: var(--target);
		height: var(--target);
		margin: calc(var(--space-2) * -1) 0;
		border-radius: var(--radius-pill);
		color: var(--text-secondary);
	}

	.round[aria-expanded='true'] {
		color: var(--link);
	}

	.round :global(svg) {
		width: 1.25rem;
		height: 1.25rem;
	}

	.controls {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	/* The pack's chip, small: on in the primary colour. */
	.toggle-chip {
		min-height: var(--space-6);
		padding: 0 var(--space-3);
		border-radius: var(--radius-pill);
		box-shadow: inset 0 0 0 1px var(--separator);
		color: var(--text);
		font-size: var(--subheadline);
		font-weight: 600;
	}

	.toggle-chip[aria-pressed='true'] {
		background: var(--accent);
		color: var(--on-accent);
		box-shadow: none;
	}

	/* Five columns, a bar of each level's colour over its name. */
	.levels {
		display: grid;
		grid-template-columns: repeat(5, minmax(0, 1fr));
		gap: 2px;
		color: var(--text-secondary);
		font-size: var(--caption);
		line-height: 1.2;
	}

	.levels span {
		display: grid;
		align-content: start;
		gap: var(--space-1);
	}

	.levels i {
		height: var(--space-3);
		box-shadow: inset 0 0 0 1px var(--color-border-strong);
	}

	.fans {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-1) var(--space-3);
		font-size: var(--caption);
		color: var(--text-secondary);
	}

	.fans span {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
	}

	/* The pack's scale legend (`.v-scale`), its two ends named; the veil beside it. */
	.key {
		display: flex;
		align-items: flex-start;
		gap: var(--space-3);
		color: var(--text-secondary);
		font-size: var(--caption);
	}

	.scale {
		flex: 1;
		display: grid;
		gap: var(--space-1);
		min-width: 0;
	}

	.bars {
		display: grid;
		grid-auto-flow: column;
		grid-auto-columns: 1fr;
		gap: 2px;
	}

	.bars i,
	.veil i {
		height: var(--space-3);
		box-shadow: inset 0 0 0 1px var(--color-border-strong);
	}

	.ends {
		display: flex;
		justify-content: space-between;
		gap: var(--space-2);
	}

	.veil {
		flex: none;
		display: grid;
		gap: var(--space-1);
		max-width: 40%;
	}

	.veil i {
		width: var(--space-6);
	}

	.stale {
		display: flex;
		align-items: flex-start;
		gap: var(--space-2);
		color: var(--color-caution);
		font-size: var(--subheadline);
		font-weight: 600;
	}

	.stale :global(svg) {
		flex: none;
		width: 1.125rem;
		height: 1.125rem;
	}

	.folded {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		scroll-margin: var(--space-1);
	}

	.folded:empty {
		display: none;
	}

	.label {
		margin: 0;
		color: var(--text-secondary);
		font-size: var(--caption);
		font-weight: 600;
	}

	.lines {
		display: grid;
		gap: var(--space-1);
		margin: 0;
		padding: 0;
		list-style: none;
		font-size: var(--subheadline);
	}

	.lines li {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	.swatch {
		flex: none;
		width: 0.875rem;
		height: 0.875rem;
		border: 1px solid var(--color-border-strong);
	}

	.caption {
		color: var(--text-secondary);
		font-size: var(--caption);
	}

	.again {
		margin-left: var(--space-1);
		color: var(--link);
		font-size: inherit;
		font-weight: 600;
	}
</style>
