<!--
	What the weather models say about one place: the wind through the day at
	the ground and higher up, the cloud base, cloud, rain and temperature,
	hour by hour. The words are those of the iPhone app's
	`ForecastScreen.swift`.

	It shows magnitudes and no verdict. Whether a day can be flown depends on
	the pilot, the wing and the place, and a model's cell of a few kilometres
	knows none of them.
-->
<script lang="ts">
	import { getApp } from '#lib/app/app.svelte.ts';
	import { type Coordinate, type ForecastView, inWordsOr } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import Segmented from '#lib/design/Segmented.svelte';
	import ForecastTable from './ForecastTable.svelte';
	import {
		caveat,
		dayName,
		daysWithLight,
		type OpenOn,
		openingOn,
		THERMALS_NOTE,
		today
	} from './table';

	const app = getApp();

	interface Props {
		/** The place, and what it is called. */
		place: Coordinate;
		name: string;
		/** Asks the server. Nothing comes back from a site without one. */
		ask: (place: Coordinate) => Promise<ForecastView | undefined>;
		/**
		 * The day of the thermal map a square was chosen on, for a forecast
		 * opened from it: the forecast opens on that day, of the model the
		 * map read it from where it can.
		 */
		openOn?: OpenOn;
	}

	let { place, name, ask, openOn }: Props = $props();

	const text = format();
	const now = Math.floor(Date.now() / 1000);

	type Phase =
		{ is: 'loading' } | { is: 'loaded'; forecast: ForecastView } | { is: 'failed'; why: string };

	let phase = $state.raw<Phase>({ is: 'loading' });
	/** Counts the requests, so that a slow answer for another place is dropped. */
	let asked = 0;
	/** Whether a forecast has come: the day it opens on is chosen only then. */
	let opened = false;

	function load(place: Coordinate) {
		const mine = ++asked;
		phase = { is: 'loading' };
		ask(place).then(
			(forecast) => {
				if (mine !== asked) return;
				if (forecast && !opened) {
					// Once, on the first answer: after it the reader's own
					// choice of model and day stands.
					opened = true;
					const start = openOn && openingOn(forecast.models, openOn);
					if (start) {
						chosenModel = start.model.model;
						chosenDay = dayName(start.day, now, text);
					}
				}
				phase = forecast
					? { is: 'loaded', forecast }
					: {
							is: 'failed',
							why: 'This copy of the app is served without Venti behind it, so there is nothing to ask.'
						};
			},
			(error) => {
				if (mine !== asked) return;
				phase = {
					is: 'failed',
					why: inWordsOr('The forecast could not be loaded.', error)
				};
			}
		);
	}

	$effect(() => load(place));

	/**
	 * The model looked at, by its identifier. The finest one at first, or
	 * the one of the thermal map's day it was opened from.
	 */
	let chosenModel = $state<string>();
	/** The day looked at, by its name. Today at first, or the thermal map's day. */
	let chosenDay = $state('');

	const models = $derived(phase.is === 'loaded' ? phase.forecast.models : []);
	const model = $derived(models.find((model) => model.model === chosenModel) ?? models[0]);
	const days = $derived(model ? daysWithLight(model) : []);
	const names = $derived(days.map((day) => dayName(day, now, text)));
	const day = $derived(days[names.indexOf(chosenDay)] ?? today(days, now));

	// The control shows the day that is looked at, also when it was not
	// chosen: at first, and when another model lacks the day that was.
	$effect(() => {
		chosenDay = day ? dayName(day, now, text) : '';
	});
</script>

<div class="forecast">
	<p class="place">{name}</p>

	{#if phase.is === 'loading'}
		<p class="waiting" role="status">Loading…</p>
	{:else if phase.is === 'failed'}
		<div class="nothing">
			<h3>No forecast</h3>
			<p>{phase.why}</p>
			<button class="again" type="button" onclick={() => load(place)}
				>{app.said?.common.tryAgain}</button
			>
		</div>
	{:else if !model}
		<div class="nothing">
			<h3>No forecast for this place</h3>
			<p>None of Venti’s weather models reaches it.</p>
		</div>
	{:else}
		{#if days.length > 1}
			<Segmented label="Day" options={names} bind:value={chosenDay} />
		{/if}

		{#if day}
			<section>
				<h3>Wind in km/h, and the sky</h3>
				<div class="card table">
					<ForecastTable {model} {day} />
				</div>
				<p class="footnote">{caveat(model, text)}</p>
				{#if day.hours.some((hour) => hour.thermalClimbMs !== undefined)}
					<p class="footnote">{THERMALS_NOTE}</p>
				{/if}
			</section>
		{/if}

		<section>
			<dl class="card">
				<!-- One model at a time, and never a blend of them. -->
				<div class="row">
					{#if models.length > 1}
						<dt><label for="forecast-model">Weather model</label></dt>
						<dd>
							<select
								id="forecast-model"
								value={model.model}
								onchange={(event) => (chosenModel = event.currentTarget.value)}
							>
								{#each models as each (each.model)}
									<option value={each.model}>{each.name}</option>
								{/each}
							</select>
						</dd>
					{:else}
						<dt>Weather model</dt>
						<dd>{model.name}</dd>
					{/if}
				</div>
				<div class="row">
					<dt>Model run</dt>
					<dd class="numeric">{text.day(model.runUnixS)}, {text.time(model.runUnixS)}</dd>
				</div>
				{#if model.cellElevationM !== undefined}
					<div class="row">
						<dt>The model’s ground here</dt>
						<dd class="numeric">{text.metres(model.cellElevationM)} m</dd>
					</div>
				{/if}
			</dl>
			<p class="footnote">
				{#each phase.forecast.credits as credit (credit.source)}
					<a href={credit.url} target="_blank" rel="noopener noreferrer">{credit.text}</a>
				{/each}
			</p>
		</section>
	{/if}
</div>

<style>
	/* As tall while it waits or has nothing as with its table: the sheet does
	   not jump when the answer comes, and on a phone it covers the site's or
	   the station's sheet it was opened from. */
	.forecast {
		display: flex;
		flex-direction: column;
		gap: 1.25rem;
		min-height: min(36rem, 72dvh);
	}

	.place {
		padding: 0 1rem;
		font-weight: 600;
		overflow-wrap: anywhere;
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
		overflow: hidden;
	}

	.table {
		padding: 0.625rem 1rem;
	}

	.row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		min-height: var(--target);
		padding: 0.5rem 1rem;
	}

	.row + .row {
		box-shadow: inset 0 0.5px 0 var(--separator);
	}

	dd {
		color: var(--text-secondary);
		text-align: right;
	}

	select {
		min-height: 2.25rem;
		padding: 0 0.5rem;
		border: 0;
		border-radius: var(--radius-control);
		background: var(--fill);
		color: var(--text);
		font: inherit;
	}

	.footnote {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		padding: 0.5rem 1rem 0;
		font-size: var(--footnote);
		color: var(--text-secondary);
		overflow-wrap: anywhere;
	}

	.footnote a {
		color: var(--link);
	}

	.waiting {
		display: grid;
		place-items: center;
		min-height: 7.5rem;
		color: var(--text-secondary);
	}

	.nothing {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.375rem;
		padding: 2rem 1.5rem;
		text-align: center;
	}

	.nothing h3 {
		padding: 0;
		font-size: var(--title);
		font-weight: 600;
		color: var(--text);
	}

	.nothing p {
		color: var(--text-secondary);
	}

	.again {
		min-height: var(--target);
		padding: 0 0.75rem;
		border-radius: var(--radius-control);
		color: var(--link);
	}

	@media (hover: hover) {
		.again:hover {
			background: var(--fill);
		}

		.footnote a:hover {
			text-decoration: underline;
		}
	}
</style>
