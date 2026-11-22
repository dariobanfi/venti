<!--
	One wind station: what it measures now, how the last 24 hours went, and
	whose data it is. The words are those of the iPhone app's
	`StationScreen.swift`.
-->
<script lang="ts">
	import {
		isInWords,
		type StationAir,
		type StationDay,
		type StationDays,
		type StationHistory,
		type WindStation
	} from '#lib/core/index.ts';
	import { age, compassPoint, format } from '#lib/design/format.ts';
	import AlertMe from '#lib/alerts/AlertMe.svelte';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { windDraft } from '#lib/alerts/model.svelte.ts';
	import Metric from '#lib/design/Metric.svelte';
	import type { WindModel } from './model.svelte';
	import WindChart from './WindChart.svelte';

	interface Props {
		station: WindStation;
		wind: WindModel;
		/** Opens the forecast for the station's place. */
		onforecast: () => void;
	}

	let { station, wind, onforecast }: Props = $props();

	/** The heading over what else is known of the air here, in the core's words. */
	const moreAir = $derived(getApp().said?.stationSheet.moreAirTitle ?? '');

	const text = format();

	type Phase =
		{ is: 'loading' } | { is: 'ready'; history: StationHistory } | { is: 'failed'; why: string };

	let phase = $state.raw<Phase>({ is: 'loading' });
	/** Counts the requests, so that a slow answer for another station is dropped. */
	let asked = 0;

	function load(id: string) {
		const mine = ++asked;
		phase = { is: 'loading' };
		wind.history(id).then(
			(history) => {
				if (mine === asked) phase = { is: 'ready', history };
			},
			(error) => {
				if (mine !== asked) return;
				const said = error instanceof Error ? error.message : String(error);
				phase = {
					is: 'failed',
					why: isInWords(error) ? said : `The history could not be loaded. ${said}`
				};
			}
		);
	}

	// The history belongs to the station, not to its newest reading: it is
	// fetched when another station is chosen, not every minute.
	const id = $derived(station.id);
	$effect(() => load(id));

	/** What the readings say about the air; asked again with every new reading. */
	let air = $state.raw<StationAir>();
	$effect(() => {
		const now = station;
		air = undefined;
		wind.air(now).then(
			(said) => {
				if (now === station) air = said;
			},
			() => {}
		);
	});

	/** The days that can be chosen, back from today. */
	let days = $state.raw<StationDays>();
	$effect(() => {
		void wind.days().then(
			(range) => (days = range),
			() => {}
		);
	});

	/** A past day chosen, by its date; nothing for the last 24 hours. */
	let chosenDate = $state<string>();
	type DayPhase =
		{ is: 'loading' } | { is: 'ready'; day: StationDay } | { is: 'failed'; why: string };
	let dayPhase = $state.raw<DayPhase>({ is: 'loading' });
	let dayAsked = 0;

	function loadDay(stationId: string, date: string) {
		const mine = ++dayAsked;
		dayPhase = { is: 'loading' };
		wind.day(stationId, date).then(
			(day) => {
				if (mine === dayAsked) dayPhase = { is: 'ready', day };
			},
			(error) => {
				if (mine !== dayAsked) return;
				const said = error instanceof Error ? error.message : String(error);
				dayPhase = {
					is: 'failed',
					why: isInWords(error) ? said : `The day could not be loaded. ${said}`
				};
			}
		);
	}

	// Another station starts again at its last 24 hours.
	$effect(() => {
		void id;
		chosenDate = undefined;
	});
	$effect(() => {
		if (chosenDate) loadDay(id, chosenDate);
	});

	/** The day picker's choice: today stands for the last 24 hours. */
	function pickDay(value: string) {
		chosenDate = !value || value === days?.lastDate ? undefined : value;
	}

	const reading = $derived(station.latest);
	const dash = '–';

	/** How old the reading is, and what that means if it is old. */
	const ageText = $derived.by(() => {
		if (!reading || station.ageMin === undefined) {
			return 'Venti has no reading from this station.';
		}
		const measured = `Measured ${age(station.ageMin)}.`;
		return station.isOld ? `${measured} Too old to say what the wind is doing now.` : measured;
	});

	/** The source's credit: the history's own, or the one that came with the map. */
	const credit = $derived(
		(phase.is === 'ready' ? phase.history.credits[0] : undefined) ??
			wind.state.credits.find((credit) => credit.source === station.source)
	);

	/** The site a credit links to, without the rest of its address. */
	function host(url: string): string {
		try {
			return new URL(url).host;
		} catch {
			return url;
		}
	}
</script>

<div class="station">
	<section aria-label="Now">
		<dl class="card now">
			<Metric
				caption="Wind"
				value={reading?.speedMs === undefined ? dash : text.kilometresPerHour(reading.speedMs)}
				unit="km/h"
			/>
			<Metric
				caption="Gusts"
				value={reading?.gustMs === undefined ? dash : text.kilometresPerHour(reading.gustMs)}
				unit="km/h"
			/>
			<Metric
				caption="From"
				value={reading?.directionDeg === undefined ? dash : compassPoint(reading.directionDeg)}
				unit={reading?.directionDeg === undefined ? '' : `${text.metres(reading.directionDeg)}°`}
			/>
			<Metric
				caption="Temperature"
				value={reading?.temperatureC === undefined ? dash : text.temperature(reading.temperatureC)}
				unit="°C"
			/>
			<Metric
				caption="Height"
				value={station.elevationM === undefined ? dash : text.metres(station.elevationM)}
				unit="m"
			/>
			<Metric caption="Measured" value={reading ? text.time(reading.timeUnixS) : dash} />
		</dl>
		<p class="footnote">{ageText}</p>
	</section>

	{#if air?.cloudBase || air?.temperatureStep}
		{@const base = air.cloudBase}
		{@const step = air.temperatureStep}
		<section aria-label={moreAir}>
			<h3>{moreAir}</h3>
			<div class="card air">
				{#if base}
					<dl class="base">
						<Metric caption={base.words} value={text.metres(base.baseM)} unit="m" />
					</dl>
					<p class="small">{base.note}</p>
				{/if}
				{#if step}
					<div class="step">
						<p>
							{step.otherName}, {text.kilometres(step.distanceM)} km away and
							<span class="numeric">{text.metres(Math.abs(step.higherByM))} m</span>
							{step.higherByM >= 0 ? 'higher' : 'lower'}:
							<span class="numeric">{text.perHundredMetres(step.per100mC)} °C</span> per 100 m
						</p>
						<p class="words">{step.words}</p>
						<p class="small">{step.rule}</p>
						<p class="small">{step.note}</p>
					</div>
				{/if}
			</div>
		</section>
	{/if}

	<section>
		<div class="history-head">
			<h3 id="history-heading">
				{#if chosenDate && dayPhase.is === 'ready'}{text.day(
						dayPhase.day.startUnixS
					)}{:else if chosenDate}A past day{:else}Last 24 hours{/if}
			</h3>
			{#if days}
				<!-- What the picker reaches back to is its own help: said under
				     a pointer and to a screen reader, not as a paragraph. -->
				<label class="pick" title={days.note}>
					<span class="visually-hidden">Day</span>
					<input
						type="date"
						aria-describedby="history-days-note"
						min={days.firstDate}
						max={days.lastDate}
						value={chosenDate ?? days.lastDate}
						onchange={(event) => pickDay(event.currentTarget.value)}
					/>
				</label>
			{/if}
		</div>
		{#if days}<p class="visually-hidden" id="history-days-note">{days.note}</p>{/if}
		<div class="card history">
			{#if chosenDate}
				{#if dayPhase.is === 'loading'}
					<p class="waiting" role="status">Loading…</p>
				{:else if dayPhase.is === 'failed'}
					<p>{dayPhase.why}</p>
					<button class="again" type="button" onclick={() => chosenDate && loadDay(id, chosenDate)}
						>Try again</button
					>
				{:else if dayPhase.day.emptyWords}
					<p>{dayPhase.day.emptyWords}</p>
				{:else if dayPhase.day.readings.length < 2}
					<p>Venti has too few readings from this day for a chart.</p>
				{:else}
					<WindChart
						readings={dayPhase.day.readings}
						from={dayPhase.day.startUnixS}
						until={dayPhase.day.endUnixS}
						span="on {text.day(dayPhase.day.startUnixS)}"
					/>
				{/if}
			{:else if phase.is === 'loading'}
				<p class="waiting" role="status">Loading…</p>
			{:else if phase.is === 'failed'}
				<p>{phase.why}</p>
				<button class="again" type="button" onclick={() => load(station.id)}>Try again</button>
			{:else if phase.history.readings.length < 2}
				<p>Venti has no history for this station yet.</p>
			{:else}
				<WindChart readings={phase.history.readings} />
			{/if}
		</div>
	</section>

	<!-- What the station measures is the wind now; what the models compute
	     for its place is another screen. -->
	<button class="card forecast" type="button" onclick={onforecast}>Forecast for this place</button>

	<AlertMe look="card" start={() => windDraft(station)} />

	{#if credit}
		<section>
			<h3>Source</h3>
			<div class="card source">
				<p>{credit.text}</p>
				<p class="lesser">{credit.provider}, under {credit.licence}.</p>
				<a class="lesser" href={credit.url} target="_blank" rel="noopener noreferrer">
					{host(credit.url)}
				</a>
			</div>
		</section>
	{/if}
</div>

<style>
	.station {
		display: flex;
		flex-direction: column;
		gap: 1.5rem;
	}

	h3 {
		padding: 0 1rem 0.375rem;
		font-size: var(--subheadline);
		font-weight: 600;
		color: var(--text-secondary);
	}

	.card {
		background: var(--surface);
		border-radius: var(--radius-card);
		padding: 1rem;
	}

	/* Six numbers, three to a row. */
	.now {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: 1rem 0.75rem;
	}

	.footnote {
		padding: 0.5rem 1rem 0;
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	.history p {
		color: var(--text-secondary);
	}

	.air {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}

	.step {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}

	.words {
		font-weight: 600;
	}

	.small {
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	.history-head {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: 0.5rem;
		padding-right: 1rem;
	}

	.history-head h3 {
		padding-bottom: 0;
	}

	.pick input {
		min-height: var(--target);
		padding: 0 0.5rem;
		border: 0;
		border-radius: var(--radius-control);
		background: var(--fill);
		color: var(--text);
		font: inherit;
	}

	.waiting {
		display: grid;
		place-items: center;
		min-height: 7.5rem;
	}

	.again {
		margin: 0.5rem 0 0 -0.75rem;
		min-height: var(--target);
		padding: 0 0.75rem;
		border-radius: var(--radius-control);
		color: var(--link);
	}

	.forecast {
		min-height: var(--target);
		padding: 0.5rem 1rem;
		color: var(--link);
		text-align: left;
	}

	.source {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 0.25rem;
		overflow-wrap: anywhere;
	}

	.lesser {
		font-size: var(--subheadline);
		color: var(--text-secondary);
	}

	a.lesser {
		color: var(--link);
	}

	@media (hover: hover) {
		a.lesser:hover {
			text-decoration: underline;
		}

		.again:hover {
			background: var(--fill);
		}

		.forecast:hover {
			background: color-mix(in srgb, var(--surface), var(--text) 6%);
		}
	}
</style>
