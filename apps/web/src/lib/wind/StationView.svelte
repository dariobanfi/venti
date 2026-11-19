<!--
	One wind station: what it measures now, how the last 24 hours went, and
	whose data it is. The words are those of the iPhone app's
	`StationScreen.swift`.
-->
<script lang="ts">
	import { isInWords, type StationHistory, type WindStation } from '#lib/core/index.ts';
	import { age, compassPoint, format } from '#lib/design/format.ts';
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

	const reading = $derived(station.latest);
	const dash = '–';

	/** How old the reading is, and what that means if it is old. */
	const ageText = $derived.by(() => {
		if (!reading || station.ageMin === undefined) {
			return 'The server has no reading from this station.';
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

	<section>
		<h3>Last 24 hours</h3>
		<div class="card history">
			{#if phase.is === 'loading'}
				<p class="waiting" role="status">Loading…</p>
			{:else if phase.is === 'failed'}
				<p>{phase.why}</p>
				<button class="again" type="button" onclick={() => load(station.id)}>Try again</button>
			{:else if phase.history.readings.length < 2}
				<p>The server has no history for this station yet.</p>
			{:else}
				<WindChart readings={phase.history.readings} />
			{/if}
		</div>
	</section>

	<!-- What the station measures is the wind now; what the models compute
	     for its place is another screen. -->
	<button class="card forecast" type="button" onclick={onforecast}>Forecast for this place</button>

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
		color: var(--accent);
	}

	.forecast {
		min-height: var(--target);
		padding: 0.5rem 1rem;
		color: var(--accent);
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
		color: var(--accent);
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
