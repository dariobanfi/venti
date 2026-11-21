<!--
	The logbook as a whole: the last twelve months, the months of all years
	together, year beside year, a typical flight, the kinds of flight, the
	gliders flown and the pilot's best flights. All of it is worked out by
	the core; the words are those of the iPhone app's `StatisticsScreen.swift`.

	On a phone it is one column of cards. On a wide screen it is a page of
	tables and two small charts side by side.
-->
<script lang="ts">
	import { getApp } from '#lib/app/app.svelte.ts';
	import { flightHref, logbookHref } from '#lib/app/routes.ts';
	import { MediaQuery } from 'svelte/reactivity';
	import type {
		BestFlight,
		LogbookStatistics,
		RouteKind,
		RouteStatistics,
		YearStatistics
	} from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import Segmented from '#lib/design/Segmented.svelte';
	import MonthChart from '#lib/logbook/MonthChart.svelte';

	const app = getApp();
	const text = format();
	const wide = new MediaQuery('min-width: 56rem', false);

	/** What a month of either chart holds. */
	type Month = { flights: number; airtimeS: number; distanceM: number };

	/** What the bars of the chart can stand for. */
	const measures = [
		{
			name: 'Airtime',
			unit: 'hours',
			value: (month: Month) => month.airtimeS / 3600,
			write: (hours: number) => text.number(Math.round(hours * 10) / 10)
		},
		{
			name: 'Distance',
			unit: 'kilometres',
			value: (month: Month) => month.distanceM / 1000,
			write: (kilometres: number) => text.count(Math.round(kilometres))
		},
		{
			name: 'Flights',
			unit: 'flights',
			value: (month: Month) => month.flights,
			write: (flights: number) => text.count(flights)
		}
	];

	let shown = $state('Airtime');
	const measure = $derived(measures.find((measure) => measure.name === shown) ?? measures[0]);

	let statistics = $state.raw<LogbookStatistics>();

	// Worked out by the core, and again whenever the logbook changes, also
	// in another tab.
	$effect(() => {
		const logbook = app.logbook;
		if (!logbook?.view) return;
		let current = true;
		void logbook.statistics().then((worked) => {
			if (current) statistics = worked;
		});
		return () => {
			current = false;
		};
	});

	const KIND: Record<RouteKind, string> = {
		freeFlight: 'Free flight',
		freeTriangle: 'Triangle',
		faiTriangle: 'FAI triangle'
	};

	/** A year in a line, for a phone: "12 flights · 340.2 km · longest 4:12 h · highest 2950 m". */
	function yearWords(year: YearStatistics): string {
		const parts = [text.flights(year.flights)];
		if (year.distanceM > 0) parts.push(`${text.kilometres(year.distanceM)} km`);
		parts.push(`longest ${text.hoursMinutes(year.longestS)} h`);
		parts.push(`highest ${text.metres(year.highestM)} m`);
		return parts.join(' · ');
	}

	/** A kind of flight in a line: "12 flights · 3 closed · farthest 44.3 km on Sun, 31 Aug 2025". */
	function routeWords(route: RouteStatistics, farthest: BestFlight): string {
		const parts = [text.flights(route.flights)];
		if (route.closed > 0) parts.push(`${route.closed} closed`);
		parts.push(
			`farthest ${text.kilometres(farthest.value)} km on ${text.day(farthest.startUnixS)}`
		);
		return parts.join(' · ');
	}

	type Best = { title: string; flight: BestFlight; value: string };

	const bests = $derived.by(() => {
		if (!statistics) return [];
		const rows: [string, BestFlight | undefined, (value: number) => string][] = [
			['Longest flight', statistics.longest, (seconds) => `${text.hoursMinutes(seconds)} h`],
			['Farthest', statistics.farthest, (metres) => `${text.kilometres(metres)} km`],
			['Most points', statistics.mostPoints, (points) => text.points(points)],
			['Highest', statistics.highest, (metres) => `${text.metres(metres)} m`],
			['Best climb', statistics.bestClimb, (rate) => `${text.climb(rate)} m/s`]
		];
		return rows.flatMap(([title, flight, write]): Best[] =>
			flight ? [{ title, flight, value: write(flight.value) }] : []
		);
	});
</script>

<svelte:head>
	<title>Statistics · Venti</title>
</svelte:head>

<div class="page">
	<header>
		<a class="back" href={logbookHref()}>
			<Icon name="back" />
			Logbook
		</a>
		<h1>Statistics</h1>
	</header>

	{#if statistics && statistics.months.length > 0}
		<div class="measures">
			<Segmented
				label="Shown"
				options={measures.map((measure) => measure.name)}
				bind:value={shown}
			/>
		</div>

		<div class="dashboard" class:wide={wide.current}>
			<section class="months" aria-labelledby="months">
				<h2 id="months">Last twelve months</h2>
				<div class="card chart">
					<MonthChart
						months={statistics.months}
						value={measure.value}
						write={measure.write}
						label="{measure.name} by month, in {measure.unit}"
					/>
				</div>
				<p class="footnote">In {measure.unit}.</p>
			</section>

			{#if statistics.seasons.length > 0}
				<section class="seasons" aria-labelledby="seasons">
					<h2 id="seasons">Through the year</h2>
					<div class="card chart">
						<MonthChart
							months={statistics.seasons}
							value={measure.value}
							write={measure.write}
							label="{measure.name} by month of the year, in {measure.unit}"
						/>
					</div>
					<p class="footnote">January to December, all years together, in {measure.unit}.</p>
				</section>
			{/if}

			{#if statistics.years.length > 0}
				<section class="years" aria-labelledby="years">
					<h2 id="years">Years</h2>
					{#if wide.current}
						<div class="card">
							<table class="numeric" aria-labelledby="years">
								<thead>
									<tr>
										<th scope="col">Year</th>
										<th scope="col">Flights</th>
										<th scope="col">Airtime</th>
										<th scope="col">Distance</th>
										<th scope="col">Points</th>
										<th scope="col">Longest</th>
										<th scope="col">Farthest</th>
										<th scope="col">Highest</th>
									</tr>
								</thead>
								<tbody>
									{#each statistics.years as year (year.year)}
										<tr>
											<th scope="row">{year.year}</th>
											<td>{text.count(year.flights)}</td>
											<td>{text.hoursMinutes(year.airtimeS)} h</td>
											<td>{text.kilometres(year.distanceM)} km</td>
											<td>{text.points(year.points)}</td>
											<td>{text.hoursMinutes(year.longestS)} h</td>
											<td>{text.kilometres(year.farthestM)} km</td>
											<td>{text.metres(year.highestM)} m</td>
										</tr>
									{/each}
								</tbody>
							</table>
						</div>
					{:else}
						<ul class="card numeric" aria-labelledby="years">
							{#each statistics.years as year (year.year)}
								<li class="row">
									<span class="pair">
										<span>{year.year}</span>
										<span class="lesser">{yearWords(year)}</span>
									</span>
									<span class="value">{text.hoursMinutes(year.airtimeS)} h</span>
								</li>
							{/each}
						</ul>
					{/if}
				</section>
			{/if}

			{#if statistics.typical}
				{@const typical = statistics.typical}
				<section class="typical" aria-labelledby="typical">
					<h2 id="typical">A typical flight</h2>
					<dl class="card numeric">
						<div class="row">
							<dt>Airtime</dt>
							<dd>{text.hoursMinutes(typical.airtimeS)} h</dd>
						</div>
						{#if typical.distanceM !== undefined}
							<div class="row">
								<dt>Distance</dt>
								<dd>{text.kilometres(typical.distanceM)} km</dd>
							</div>
						{/if}
						<div class="row">
							<dt>Highest</dt>
							<dd>{text.metres(typical.highestM)} m</dd>
						</div>
						<div class="row">
							<dt>Best climb</dt>
							<dd>{text.climb(typical.bestClimbMs)} m/s</dd>
						</div>
					</dl>
					<p class="footnote">
						The middle one of all your flights for each figure, so that one long day does not speak
						for the rest.
					</p>
				</section>
			{/if}

			{#if statistics.routes.some((route) => route.farthest)}
				<section class="routes" aria-labelledby="routes">
					<h2 id="routes">Kinds of flight</h2>
					<ul class="card numeric" aria-labelledby="routes">
						{#each statistics.routes as route (route.kind)}
							{#if route.farthest}
								<li>
									<a class="row" href={flightHref(route.farthest.flightId)}>
										<span class="pair">
											<span>{KIND[route.kind]}</span>
											<span class="lesser">{routeWords(route, route.farthest)}</span>
										</span>
										<span class="value">{text.kilometres(route.distanceM)} km</span>
										<span class="chevron"><Icon name="forward" /></span>
									</a>
								</li>
							{/if}
						{/each}
					</ul>
				</section>
			{/if}

			{#if statistics.gliders.length > 0}
				<section class="gliders" aria-labelledby="gliders">
					<h2 id="gliders">Gliders</h2>
					<dl class="card numeric">
						{#each statistics.gliders as glider (glider.glider)}
							<div class="row">
								<dt>{glider.glider ?? 'Not named'}</dt>
								<dd>{text.flights(glider.flights)} · {text.hoursMinutes(glider.airtimeS)} h</dd>
							</div>
						{/each}
					</dl>
				</section>
			{/if}

			{#if bests.length > 0}
				<section class="bests" aria-labelledby="bests">
					<h2 id="bests">Your bests</h2>
					<ul class="card numeric" aria-labelledby="bests">
						{#each bests as best (best.title)}
							<li>
								<a class="row" href={flightHref(best.flight.flightId)}>
									<span class="pair">
										<span>{best.title}</span>
										<span class="lesser">{text.day(best.flight.startUnixS)}</span>
									</span>
									<span class="value">{best.value}</span>
									<span class="chevron"><Icon name="forward" /></span>
								</a>
							</li>
						{/each}
					</ul>
				</section>
			{/if}
		</div>
	{:else if statistics}
		<p class="nothing">There is nothing to count yet. Statistics appear with the first flight.</p>
	{/if}
</div>

<style>
	.page {
		max-width: 44rem;
		margin: 0 auto;
		padding: calc(0.5rem + var(--safe-top)) calc(var(--gutter) + var(--safe-right))
			calc(1.5rem + var(--nav-bottom)) calc(var(--gutter) + var(--safe-left));
	}

	.back {
		display: inline-flex;
		align-items: center;
		gap: 0.125rem;
		min-height: var(--target);
		margin-left: -0.5rem;
		padding: 0 0.75rem 0 0.25rem;
		border-radius: var(--radius-control);
		color: var(--link);
	}

	h1 {
		margin-bottom: 0.75rem;
		font-size: var(--large-title);
		font-weight: 600;
	}

	section {
		margin-top: 1.5rem;
	}

	h2 {
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

	.chart {
		padding: 1rem;
	}

	.measures {
		max-width: 24rem;
		margin-top: var(--space-2);
	}

	/* Wide: a page of tables and two small charts side by side. */
	.dashboard.wide {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		column-gap: var(--space-5);
		align-items: start;
	}

	.wide .years {
		grid-column: 1 / -1;
	}

	.wide section {
		margin-top: var(--space-4);
	}

	table {
		width: 100%;
		border-collapse: collapse;
	}

	th,
	td {
		height: 2rem;
		padding: 0 var(--space-3);
		text-align: right;
		white-space: nowrap;
	}

	thead th {
		font-size: var(--subheadline);
		font-weight: 600;
		color: var(--text-secondary);
		box-shadow: inset 0 -1px 0 var(--separator);
	}

	th:first-child {
		text-align: left;
	}

	tbody tr + tr {
		box-shadow: inset 0 1px 0 var(--separator);
	}

	.row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		min-height: var(--target);
		padding: 0.5rem 1rem;
	}

	.row + .row,
	li + li {
		box-shadow: inset 0 0.5px 0 var(--separator);
	}

	dd,
	.value {
		color: var(--text-secondary);
		text-align: right;
		white-space: nowrap;
	}

	a.row {
		padding-right: 0.75rem;
		gap: 0.5rem;
	}

	.pair {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
		min-width: 0;
	}

	.lesser {
		font-size: var(--subheadline);
		color: var(--text-secondary);
	}

	.chevron {
		color: var(--icon-quiet);
		font-size: 0.75rem;
	}

	@media (hover: hover) {
		a.row:hover,
		.back:hover {
			background: var(--fill);
		}
	}

	a.row:focus-visible {
		outline-offset: -0.1875rem;
		border-radius: 0.5rem;
	}

	.footnote {
		padding: 0.5rem 1rem 0;
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	.nothing {
		margin-top: 3rem;
		color: var(--text-secondary);
		text-align: center;
	}

	@media (min-width: 56rem) {
		.page {
			max-width: 80rem;
			padding-top: var(--space-4);
		}

		.chart {
			padding: var(--space-3);
		}

		.row {
			padding: var(--space-1) var(--space-3);
		}
	}
</style>
