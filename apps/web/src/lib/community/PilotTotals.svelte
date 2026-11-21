<!--
	What a pilot's public flights add up to, as the core gives it
	(`pilotStatistics`): the choice of all years or one, the numbers as
	labelled rows (a row about one flight opens that flight), the take-offs
	flown from most, the twelve months as the bars a site's flights have,
	and under them what the numbers are made of and the flights' terms.

	Every word and number is the core's; nothing is added up here. A
	take-off opens its place's page where one of the flights listed took
	off from it, which is where the page learns where the site is.
-->
<script lang="ts">
	import { placeHref, publicFlightHref } from '#lib/app/routes.ts';
	import type { PilotStatistics, PublicFlight } from '#lib/core/index.ts';
	import MonthBars from '#lib/design/MonthBars.svelte';
	import Segmented from '#lib/design/Segmented.svelte';

	interface Props {
		statistics: PilotStatistics;
		/** The pilot's flights listed so far: where their take-offs are. */
		flights: readonly PublicFlight[];
	}

	let { statistics, flights }: Props = $props();

	const views = $derived([statistics.overall, ...statistics.seasons]);
	const names = $derived(views.map((view) => view.name));
	/** The name of the view chosen. All years until another is. */
	let chosen = $state('');
	const view = $derived(views.find((one) => one.name === chosen) ?? statistics.overall);

	/** How many choices a row of segments has room for in a side column. */
	const SEGMENTS = 4;

	/** Where a take-off is, as one of the flights listed says. */
	function whereIs(site: string): PublicFlight['takeoff'] | undefined {
		return flights.find((flight) => flight.takeoffSiteId === site)?.takeoff;
	}

	const id = $props.id();
</script>

<section class="totals" aria-label="Totals">
	{#if names.length > 1}
		{#if names.length <= SEGMENTS}
			<Segmented
				label="Season of the totals"
				options={names}
				bind:value={() => view.name, (name) => (chosen = name)}
			/>
		{:else}
			<label class="visually-hidden" for="{id}-season">Season of the totals</label>
			<select id="{id}-season" bind:value={() => view.name, (name) => (chosen = name)}>
				{#each names as name (name)}
					<option value={name}>{name}</option>
				{/each}
			</select>
		{/if}
	{/if}

	<dl class="card" aria-label={view.words}>
		{#each view.rows as row (row.label)}
			<div class="row">
				<dt>{row.label}</dt>
				<dd class="numeric">
					{#if row.flightId}
						<a href={publicFlightHref(row.flightId)}>{row.value}</a>
					{:else}
						{row.value}
					{/if}
				</dd>
			</div>
		{/each}
	</dl>

	{#if view.sites.length > 0}
		<ul class="card" aria-label="Take-offs flown from most">
			{#each view.sites as site (site.id)}
				{@const at = whereIs(site.id)}
				<li>
					{#if at}
						<a class="row link" href={placeHref(at, site.id)}>{site.words}</a>
					{:else}
						<span class="row">{site.words}</span>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}

	<div class="card months" role="group" aria-label="Flights by month">
		<MonthBars months={view.months} />
	</div>

	{#if statistics.note}
		<p class="footnote">{statistics.note}</p>
	{/if}
	{#if statistics.terms.text}
		<p class="footnote">{statistics.terms.text}</p>
	{/if}
</section>

<style>
	.totals {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}

	.card {
		background: var(--surface);
		border-radius: var(--radius-card);
		overflow: hidden;
		list-style: none;
	}

	.row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-4);
		min-height: var(--target);
		padding: var(--space-2) var(--space-4);
		overflow-wrap: anywhere;
	}

	.row + .row,
	li + li .row {
		box-shadow: inset 0 0.5px 0 var(--separator);
	}

	dt {
		color: var(--text-secondary);
	}

	dd {
		font-weight: 600;
		text-align: right;
	}

	a {
		color: var(--link);
	}

	.link {
		width: 100%;
	}

	@media (hover: hover) {
		.link:hover {
			background: var(--fill);
		}
	}

	.link:focus-visible {
		outline-offset: -0.1875rem;
		border-radius: var(--radius-card);
	}

	.months {
		padding: var(--space-3) var(--space-4);
	}

	select {
		min-height: var(--target);
		padding: 0 var(--space-3);
		border: 0;
		border-radius: var(--radius-control);
		background: var(--fill);
		color: var(--text);
		font: inherit;
	}

	.footnote {
		padding: 0 var(--space-4);
		font-size: var(--footnote);
		color: var(--text-secondary);
		overflow-wrap: anywhere;
	}

	@media (min-width: 56rem) {
		.card {
			border: 1px solid var(--color-border);
			border-radius: var(--radius-m);
		}
	}
</style>
