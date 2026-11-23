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
	import { getApp } from '#lib/app/app.svelte.ts';
	import type { PilotStatistics, PilotTotalsView } from '#lib/core/index.ts';
	import MonthBars from '#lib/design/MonthBars.svelte';
	import Segmented from '#lib/design/Segmented.svelte';

	interface Props {
		statistics: PilotStatistics;
		/** Told the season chosen: the page narrows the pilot's list to it. */
		onseason?: (season: PilotTotalsView) => void;
		/** Where a row about one flight leads: its public page, or the pilot's own one. */
		flightHref?: (id: string) => string;
		/**
		 * Whether the totals are the page and not a column beside a list: on
		 * a wide screen the numbers then stand beside the sites and months.
		 */
		whole?: boolean;
		/**
		 * Whether these are the flights the pilot shares with everyone, which
		 * the core's note under them says. Not the pilot's own statistics,
		 * which count the private flights too: the note would be wrong there.
		 */
		shared?: boolean;
	}

	let {
		statistics,
		onseason,
		flightHref = publicFlightHref,
		whole = false,
		shared = true
	}: Props = $props();

	/** The headings and the choice's name are the core's (`pilotsWords`). */
	const said = $derived(getApp().everyone?.pilotsWords);

	const views = $derived([statistics.overall, ...statistics.seasons]);
	const names = $derived(views.map((view) => view.name));
	/** The name of the view chosen. All years until another is. */
	let chosen = $state('');
	const view = $derived(views.find((one) => one.name === chosen) ?? statistics.overall);

	/** How many choices a row of segments has room for in a side column. */
	const SEGMENTS = 4;

	function choose(name: string) {
		chosen = name;
		const season = views.find((one) => one.name === name);
		if (season) onseason?.(season);
	}

	const id = $props.id();
</script>

<section class="totals" class:whole aria-label="Totals">
	{#if names.length > 1}
		{#if names.length <= SEGMENTS}
			<Segmented
				label={said?.seasonChoice ?? ''}
				options={names}
				bind:value={() => view.name, choose}
			/>
		{:else}
			<label class="visually-hidden" for="{id}-season">{said?.seasonChoice ?? ''}</label>
			<select id="{id}-season" bind:value={() => view.name, choose}>
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
						<a href={flightHref(row.flightId)}>{row.value}</a>
					{:else}
						{row.value}
					{/if}
				</dd>
			</div>
		{/each}
	</dl>

	{#if view.sites.length > 0}
		<ul class="card" aria-label={said?.sitesTitle}>
			{#each view.sites as site (site.id)}
				<li>
					{#if site.position}
						<a class="row link" href={placeHref(site.position, site.id)}>{site.words}</a>
					{:else}
						<span class="row">{site.words}</span>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}

	<div class="card months" role="group" aria-label={said?.monthsTitle}>
		<MonthBars months={view.months} />
	</div>

	{#if statistics.note && shared}
		<p class="footnote">{statistics.note}</p>
	{/if}
	<!-- Under what the flights are given: behind the core's title for it. -->
	{#if statistics.terms.text && statistics.terms.title}
		<details class="footnote terms">
			<summary>{statistics.terms.title}</summary>
			<p>{statistics.terms.text}</p>
		</details>
	{:else if statistics.terms.text}
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

	.terms summary {
		color: var(--link);
		cursor: pointer;
	}

	.terms p {
		margin-top: var(--space-1);
	}

	@media (min-width: 56rem) {
		.card {
			border: 1px solid var(--color-border);
			border-radius: var(--radius-m);
		}

		/* The whole page: the numbers at the left, the sites and the months
		   at the right, the notes across both. */
		.whole {
			display: grid;
			grid-template-columns: repeat(2, minmax(0, 1fr));
			align-items: start;
			gap: var(--space-4) var(--space-5);
		}

		/* The choice over the numbers, in their column. */
		.whole > :global(*) {
			grid-column: 1;
		}

		.whole > .footnote {
			grid-column: 1 / -1;
		}

		.whole > dl.card {
			grid-row: span 2;
		}

		.whole > ul.card,
		.whole > .months {
			grid-column: 2;
		}
	}
</style>
