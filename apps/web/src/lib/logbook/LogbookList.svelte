<!--
	The pilot's flights, newest first, by year. The first card holds the
	totals: flights, airtime and cross-country distance. Each year's heading
	repeats that year's count and airtime.

	Grouping, order and every sum are the core's. This only lays them out.
-->
<script lang="ts">
	import type { LogbookEntry, LogbookView } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import Metric from '#lib/design/Metric.svelte';
	import { statisticsHref } from '#lib/app/routes.ts';
	import LogbookRow from './LogbookRow.svelte';

	interface Props {
		view: LogbookView;
		ondelete: (entry: LogbookEntry) => void;
	}

	let { view, ondelete }: Props = $props();

	const text = format();
</script>

<!-- The totals lead to the statistics. -->
<a class="card totals" href={statisticsHref()} aria-label="Statistics">
	<dl aria-label="All flights">
		<Metric
			caption={view.totals.flights === 1 ? 'Flight' : 'Flights'}
			value={text.count(view.totals.flights)}
		/>
		<Metric caption="Airtime" value={text.hoursMinutes(view.totals.airtimeS)} unit="h" />
		<Metric caption="Distance" value={text.wholeKilometres(view.totals.distanceM)} unit="km" />
	</dl>
	<span class="chevron"><Icon name="forward" /></span>
</a>

{#each view.years as year (year.year)}
	<section aria-labelledby="year-{year.year}">
		<header>
			<h2 id="year-{year.year}">{year.year}</h2>
			<p class="numeric">
				{text.flights(year.totals.flights)} · {text.hoursMinutes(year.totals.airtimeS)} h
			</p>
		</header>
		<div class="card">
			<!-- What the columns of a wide screen hold. Each number says so
			     itself to a screen reader, so the headings are for the eye. -->
			<div class="columns" aria-hidden="true">
				<span>Airtime</span>
				<span>Distance</span>
				<span>Highest</span>
				<span>Best climb</span>
			</div>
			<ul>
				{#each year.flights as entry (entry.id)}
					<LogbookRow {entry} {ondelete} />
				{/each}
			</ul>
		</div>
	</section>
{/each}

<style>
	.card {
		background: var(--surface);
		border-radius: var(--radius-card);
		overflow: hidden;
		/* The width of one column of numbers on a wide screen. */
		--column: 7rem;
	}

	.totals {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 1rem 0.75rem 1rem 1.25rem;
	}

	.totals dl {
		flex: 1;
		display: flex;
		gap: 0.75rem;
	}

	.chevron {
		color: var(--icon-quiet);
		font-size: 0.75rem;
	}

	@media (hover: hover) {
		.totals:hover {
			background: color-mix(in srgb, var(--surface), var(--text) 4%);
		}
	}

	section {
		margin-top: 1.75rem;
	}

	header {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 1rem;
		padding: 0 1rem 0.5rem;
	}

	h2 {
		font-size: var(--body);
		font-weight: 600;
	}

	header p {
		font-size: var(--subheadline);
		color: var(--text-secondary);
	}

	.columns {
		display: none;
	}

	@media (min-width: 56rem) {
		.totals {
			padding: 1.25rem 1rem 1.25rem 1.5rem;
		}

		.columns {
			display: flex;
			justify-content: flex-end;
			/* The delete button's column: its size and its margins. */
			padding: 0.625rem calc(var(--target) + 1rem) 0.375rem 1rem;
			font-size: var(--caption);
			color: var(--text-secondary);
			border-bottom: 0.5px solid var(--separator);
		}

		.columns span {
			width: var(--column);
			text-align: right;
		}
	}
</style>
