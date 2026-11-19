<!--
	The logbook as a whole: the last twelve months, the gliders flown, and the
	pilot's best flights. All of it is worked out by the core; the words are
	those of the iPhone app's `StatisticsScreen.swift`.
-->
<script lang="ts">
	import { getApp } from '#lib/app/app.svelte.ts';
	import { flightHref, logbookHref } from '#lib/app/routes.ts';
	import type { BestFlight, LogbookStatistics, MonthStatistics } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import MonthChart from '#lib/logbook/MonthChart.svelte';

	const app = getApp();
	const text = format();

	/** What the bars of the chart can stand for. */
	const measures = [
		{
			name: 'Airtime',
			unit: 'hours',
			value: (month: MonthStatistics) => month.airtimeS / 3600,
			write: (hours: number) => text.number(Math.round(hours * 10) / 10)
		},
		{
			name: 'Distance',
			unit: 'kilometres',
			value: (month: MonthStatistics) => month.distanceM / 1000,
			write: (kilometres: number) => text.count(Math.round(kilometres))
		},
		{
			name: 'Flights',
			unit: 'flights',
			value: (month: MonthStatistics) => month.flights,
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
		<section aria-labelledby="months">
			<h2 id="months">Last twelve months</h2>
			<div class="card chart">
				<div class="measures" role="radiogroup" aria-label="Shown">
					{#each measures as option (option.name)}
						<label class:chosen={shown === option.name}>
							<input type="radio" name="measure" value={option.name} bind:group={shown} />
							{option.name}
						</label>
					{/each}
				</div>
				<MonthChart
					months={statistics.months}
					value={measure.value}
					write={measure.write}
					label="{measure.name} by month, in {measure.unit}"
				/>
			</div>
			<p class="footnote">In {measure.unit}.</p>
		</section>

		{#if statistics.gliders.length > 0}
			<section aria-labelledby="gliders">
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
			<section aria-labelledby="bests">
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
		color: var(--accent);
	}

	h1 {
		margin-bottom: 0.75rem;
		font-size: var(--large-title);
		font-weight: 700;
		letter-spacing: -0.01em;
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

	/* One of three: a row of choices, the chosen one raised. */
	.measures {
		display: flex;
		gap: 0.125rem;
		margin-bottom: 1rem;
		padding: 0.125rem;
		border-radius: 0.625rem;
		background: var(--fill);
	}

	.measures label {
		flex: 1;
		display: grid;
		place-items: center;
		min-height: 2rem;
		border-radius: 0.5rem;
		font-size: var(--subheadline);
		cursor: pointer;
	}

	.measures .chosen {
		background: var(--surface);
		font-weight: 600;
		box-shadow: 0 1px 3px rgb(0 0 0 / 0.14);
	}

	@media (prefers-color-scheme: dark) {
		.measures .chosen {
			background: var(--fill-strong);
		}
	}

	/* The radio buttons themselves are not drawn, but they take the keyboard. */
	.measures input {
		position: absolute;
		opacity: 0;
	}

	.measures label:has(:focus-visible) {
		outline: 0.1875rem solid var(--accent);
		outline-offset: 0.125rem;
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
			padding-top: 1.25rem;
		}
	}
</style>
