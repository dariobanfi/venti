<!--
	A place pilots fly from: the flying site there, the flights that took
	off around it, the newest or the best (its records), and the ranking of
	their pilots. Which place is in the address, with the site the page is
	about where it was reached from one: `?lat=47.6642&lon=11.7967&site=…`.

	Two answers of the server's on one page, kept apart. The site is its
	source's record, OpenStreetMap's under a share-alike licence, and stands
	in a section of its own with that source's credit. The flights and the
	ranking are the pilots' own, asked for by where the flights took off;
	nothing of the site is put into them, and nothing of them into the site.
-->
<script lang="ts">
	import { MediaQuery } from 'svelte/reactivity';
	import { browser } from '$app/env';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { everyoneHref, mapHref, placeHref } from '#lib/app/routes.ts';
	import { NOT_RANKED } from '#lib/community/model.svelte.ts';
	import { NOT_LISTED, placeAskedIn, placeQuery, queryOf } from '#lib/community/pages.ts';
	import PublicFlightList from '#lib/community/PublicFlightList.svelte';
	import RankingList from '#lib/community/RankingList.svelte';
	import {
		nothingFromHere,
		nothingRanked,
		PLACE_APART,
		PLACE_RECKONING
	} from '#lib/community/words.ts';
	import type { Coordinate, PublicFlightsOrder } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import Segmented from '#lib/design/Segmented.svelte';
	import Sheet from '#lib/design/Sheet.svelte';
	import ForecastView from '#lib/forecast/ForecastView.svelte';
	import { keepRoute } from '#lib/route/kept.ts';
	import SiteView from '#lib/sites/SiteView.svelte';

	const app = getApp();
	const everyone = $derived(app.everyone);
	const text = format();
	const wide = new MediaQuery('min-width: 56rem', false);

	/** What the address asks for. It is not known while the pages are written out at build time. */
	const asked = $derived(browser ? placeAskedIn(page.url.searchParams) : undefined);
	const place = $derived(everyone?.place);
	const list = $derived(place?.list ?? NOT_LISTED);
	const ranking = $derived(place?.ranking ?? NOT_RANKED);
	const found = $derived(place?.site);
	const site = $derived(found?.site);

	$effect(() => {
		const at = asked?.at;
		if (!asked || !at) return;
		everyone?.showPlace({ at, site: asked.site, order: asked.order, season: asked.season });
	});

	/** Asks for another order or season: the address changes in place, and the page follows it. */
	function ask(next: { order: PublicFlightsOrder; season: string }) {
		if (!asked?.at) return;
		const season = next.season === ranking.seasons[0] ? '' : next.season;
		void goto(placeHref(asked.at, asked.site, placeQuery({ order: next.order, season })), {
			replaceState: true,
			keepFocus: true,
			noScroll: true
		});
	}

	const ORDERS = ['Newest', 'Best'] as const;

	/**
	 * "Plan route": the map, around the site, with the planner open on a
	 * new route that begins there, as the site's page on the map does it.
	 */
	function planFrom(from: NonNullable<typeof site>) {
		const { lat, lon } = from.position;
		keepRoute(globalThis.localStorage, { places: [{ lat, lon, name: from.name }], open: true });
		void goto(mapHref(from.position));
	}

	/** The place whose forecast is looked at. */
	let forecastFor = $state.raw<{ place: Coordinate; name: string }>();

	/** The site's name where its source gives one; a site without is said by what it is, under the title. */
	const title = $derived(site?.isNamed ? site.name : 'Flown from here');
	/** What is said under the title: what the site is, or that the source knows none here. */
	const subtitle = $derived.by(() => {
		if (!place?.siteKnown || place.absent) return undefined;
		if (place.siteProblem) return place.siteProblem;
		if (!site) return 'The flying sites’ source knows no site here.';
		return site.elevationM === undefined
			? site.kindInWords
			: `${site.kindInWords} · ${text.metres(site.elevationM)} m`;
	});

	/** Back to everyone's flights as they were asked for. */
	const back = $derived(everyoneHref(everyone ? queryOf(everyone.asked) : ''));
</script>

<svelte:head>
	<title>{title} · Venti</title>
</svelte:head>

<div class="page">
	<a class="back" href={back}>
		<Icon name="back" />
		Everyone
	</a>
	<header>
		<h1>{title}</h1>
		{#if subtitle}
			<p>{subtitle}</p>
		{/if}
		{#if asked?.at}
			<p class="numeric">{text.place(asked.at.lat, asked.at.lon)}</p>
		{/if}
	</header>

	{#if asked && !asked.at}
		<div class="unavailable">
			<span class="symbol"><Icon name="warning" /></span>
			<h2>This address names no place</h2>
			<p>
				A place’s flights are reached from a flying site on the map, or from where a flight took
				off.
			</p>
		</div>
	{:else if app.failure}
		<div class="unavailable">
			<span class="symbol"><Icon name="warning" /></span>
			<h2>Venti could not open its data</h2>
			<p>{app.failure}</p>
		</div>
	{:else if asked}
		<div class="columns" class:wide={wide.current}>
			<div class="main">
				<section aria-labelledby="flown-here">
					<h2 id="flown-here">Flights from here</h2>
					{#if !list.absent}
						<div class="order">
							<Segmented
								label="Order"
								options={ORDERS}
								bind:value={
									() => (asked.order === 'best' ? 'Best' : 'Newest'),
									(chosen) =>
										ask({ order: chosen === 'Best' ? 'best' : 'newest', season: asked.season })
								}
							/>
						</div>
					{/if}
					<PublicFlightList
						label="Flights from here"
						{list}
						named
						nothing={nothingFromHere(list)}
						reckoning={PLACE_RECKONING}
						onmore={() => everyone?.morePlace()}
						onagain={() => everyone?.againPlace()}
					/>
				</section>

				{#if !list.absent}
					<section aria-labelledby="ranked-here">
						<h2 id="ranked-here">Pilots from here</h2>
						{#if ranking.seasons.length > 0}
							<div class="season">
								<Segmented
									label={everyone?.pilotsWords?.seasonChoice ?? ''}
									options={ranking.seasons}
									bind:value={
										() => ranking.season, (chosen) => ask({ order: asked.order, season: chosen })
									}
								/>
							</div>
						{/if}
						{#if ranking.pilots.length > 0}
							<RankingList label="Ranking from here, {ranking.season}" pilots={ranking.pilots} />
							<p class="footnote">
								{ranking.rule} Only flights that took off within a kilometre and a half of this place
								count.
							</p>
						{:else if ranking.loading || (!ranking.known && !ranking.problem && !ranking.absent)}
							<p class="waiting" role="status">Fetching the ranking…</p>
						{:else}
							{@const nothing = nothingRanked(ranking)}
							<div class="card empty">
								<p><strong>{nothing.title}</strong></p>
								<p>{nothing.says}</p>
								{#if ranking.problem}
									<button
										class="button-plain"
										type="button"
										onclick={() => everyone?.rankPlaceAgain()}
									>
										{app.said?.common.tryAgain}
									</button>
								{/if}
							</div>
						{/if}
					</section>
				{/if}
			</div>

			{#if site}
				<section class="aside" aria-labelledby="the-site">
					<h2 id="the-site">The site</h2>
					<SiteView
						{site}
						credit={found?.credit}
						asOfUnixS={found?.asOfUnixS}
						onforecast={() => (forecastFor = { place: site.position, name: site.name })}
						onplan={() => planFrom(site)}
					/>
					<p class="footnote">{PLACE_APART}</p>
				</section>
			{/if}
		</div>
	{/if}
</div>

<!-- What the weather models say about the site's place. -->
<Sheet open={Boolean(forecastFor)} title="Forecast" onclose={() => (forecastFor = undefined)}>
	{#if forecastFor}
		<ForecastView {...forecastFor} ask={(where) => app.forecast(where)} />
	{/if}
</Sheet>

<style>
	.page {
		display: flex;
		flex-direction: column;
		min-height: calc(100dvh - var(--nav-top));
		/* The whole width of the site's column: the table's columns beside the site. */
		max-width: var(--content-max);
		margin: 0 auto;
		padding: calc(0.75rem + var(--safe-top)) calc(var(--gutter) + var(--safe-right))
			calc(1.5rem + var(--nav-bottom)) calc(var(--gutter) + var(--safe-left));
	}

	.back {
		align-self: flex-start;
		display: inline-flex;
		align-items: center;
		gap: 0.125rem;
		min-height: var(--target);
		margin-left: -0.375rem;
		padding-right: 0.5rem;
		border-radius: var(--radius-control);
		color: var(--link);
	}

	header {
		margin-bottom: 1.25rem;
	}

	h1 {
		font-size: var(--large-title);
		font-weight: 600;
		overflow-wrap: anywhere;
	}

	header p {
		color: var(--text-secondary);
	}

	.columns {
		display: flex;
		flex-direction: column;
		gap: 2rem;
	}

	.main {
		display: flex;
		flex-direction: column;
		gap: 2rem;
		min-width: 0;
	}

	/* Wide: the site stands beside the flights, which are what the page is for. */
	.columns.wide {
		display: grid;
		grid-template-columns: minmax(0, 1fr) 24rem;
		align-items: start;
	}

	h2 {
		margin: 0 0 0.625rem;
		font-size: var(--title);
		font-weight: 600;
	}

	.order,
	.season {
		max-width: 28rem;
		margin-bottom: 0.75rem;
	}

	.footnote {
		margin-top: 0.75rem;
		padding: 0 1rem;
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	.waiting {
		padding: 1.5rem 1rem;
		text-align: center;
		color: var(--text-secondary);
	}

	.card {
		background: var(--surface);
		border-radius: var(--radius-card);
	}

	.empty {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 0.25rem;
		padding: 0.875rem 1rem;
	}

	.empty p:last-of-type {
		color: var(--text-secondary);
	}

	.unavailable {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 0.375rem;
		max-width: 24rem;
		margin: 0 auto;
		padding-bottom: 3rem;
		text-align: center;
	}

	.symbol {
		color: var(--text-secondary);
		font-size: 2.25rem;
		margin-bottom: 0.5rem;
	}

	.unavailable h2 {
		margin: 0;
	}

	.unavailable p {
		color: var(--text-secondary);
	}

	@media (min-width: 56rem) {
		.page {
			padding-top: 1.5rem;
		}
	}
</style>
