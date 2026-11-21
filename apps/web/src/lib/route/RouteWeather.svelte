<!--
	The weather along the route, in the planner's card: the start, the mean
	speed over the ground and the glide, then the core's line for the whole
	route, each place's line with the height to leave it at, each leg's
	line, and whose forecast it is. Every sentence is the core's.
-->
<script lang="ts">
	import type { ThermalMapDay } from '#lib/core/index.ts';
	import type { RouteWeatherModel } from './weather.svelte';

	interface Props {
		weather: RouteWeatherModel;
		/** The thermal map's days, the first today; none while the map is not there. */
		days: readonly ThermalMapDay[];
	}

	let { weather, days }: Props = $props();

	const along = $derived(weather.along);
	const chosen = $derived(days.find((day) => day.date === weather.date) ?? days[0]);

	function number(event: Event, set: (value: number) => void) {
		const value = (event.currentTarget as HTMLInputElement).valueAsNumber;
		if (Number.isFinite(value) && value > 0) {
			set(value);
			weather.keep();
		}
	}
</script>

<div class="weather">
	<div class="settings">
		<div class="setting">
			<span>Start</span>
			<span class="start">
				{#if days.length > 0}
					<select
						aria-label="Day"
						value={chosen?.date}
						onchange={(event) => (weather.date = event.currentTarget.value)}
					>
						{#each days as day (day.date)}
							<option value={day.date}>{day.name}</option>
						{/each}
					</select>
				{/if}
				<input
					type="time"
					aria-label="Start time"
					value={weather.time}
					onchange={(event) => {
						if (event.currentTarget.value) weather.time = event.currentTarget.value;
					}}
				/>
			</span>
		</div>
		<label class="setting">
			<span>Speed, km/h</span>
			<input
				type="number"
				inputmode="decimal"
				min="5"
				max="80"
				step="1"
				value={weather.speedKmh}
				onchange={(event) => number(event, (value) => (weather.speedKmh = value))}
			/>
		</label>
		<label class="setting">
			<span>Glide</span>
			<input
				type="number"
				inputmode="decimal"
				min="2"
				max="20"
				step="0.5"
				value={weather.glide}
				onchange={(event) => number(event, (value) => (weather.glide = value))}
			/>
		</label>
	</div>

	{#if weather.problem}
		<p class="problem" role="alert">{weather.problem}</p>
	{/if}
	{#if along}
		<p class="whole" role="status" data-route-weather>{along.words}</p>
		<ol class="lines" aria-label="Along the route">
			{#each along.points as point, index (index)}
				<li>
					<p>{point.words}</p>
					{#if point.departureWords}
						<p class="lesser">{point.departureWords}</p>
					{/if}
				</li>
				{#if along.legs[index]}
					<li class="leg">{along.legs[index].words}</li>
				{/if}
			{/each}
		</ol>
		{#if along.model || along.credits.length > 0}
			<p class="credit">
				{[along.model, ...along.credits.map((credit) => credit.provider)]
					.filter(Boolean)
					.join(' · ')}
			</p>
		{/if}
	{/if}
</div>

<style>
	.weather {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding-top: var(--space-2);
		border-top: 1px solid var(--separator);
		font-size: var(--footnote);
	}

	.settings {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2) var(--space-3);
	}

	.setting {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		color: var(--text-secondary);
	}

	.start {
		display: flex;
		gap: var(--space-1);
	}

	input,
	select {
		min-height: 2rem;
		padding: 0 var(--space-2);
		border: 1px solid var(--separator);
		border-radius: var(--radius-control);
		background: var(--surface);
		color: var(--text);
		font: inherit;
	}

	input[type='number'] {
		width: 4.5rem;
	}

	.whole {
		font-size: var(--subheadline);
		color: var(--text);
	}

	.problem {
		color: var(--danger);
	}

	.lines {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.leg {
		padding-left: var(--space-3);
		color: var(--text-secondary);
	}

	.lesser,
	.credit {
		color: var(--text-secondary);
	}

	.credit {
		font-size: var(--caption);
	}
</style>
