<!--
	The ranking: the pilots of this site's Venti server by the sum of the
	points of their best flights that they share with everyone, in a year or
	in all years. Which is in the address, so the page can be come back to
	and handed on: `?season=2026`.
-->
<script lang="ts">
	import { browser } from '$app/env';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { rankingHref } from '#lib/app/routes.ts';
	import { NOT_RANKED } from '#lib/community/model.svelte.ts';
	import { queryOf } from '#lib/community/pages.ts';
	import RankingList from '#lib/community/RankingList.svelte';
	import RankingTable from '#lib/community/RankingTable.svelte';
	import { MediaQuery } from 'svelte/reactivity';
	import WhoseFlights from '#lib/community/WhoseFlights.svelte';
	import { nothingRanked } from '#lib/community/words.ts';
	import Icon from '#lib/design/Icon.svelte';
	import Segmented from '#lib/design/Segmented.svelte';

	const app = getApp();
	const everyone = $derived(app.everyone);

	/** The season the address asks for. It is not known while the pages are written out at build time. */
	const asked = $derived(browser ? (page.url.searchParams.get('season') ?? '') : '');
	const ranking = $derived(everyone?.ranking ?? NOT_RANKED);
	const nothing = $derived(nothingRanked(ranking));

	$effect(() => everyone?.showRanking(asked));

	/**
	 * Asks for another season: the address changes in place, with the
	 * keyboard and the scroll where they are, and the ranking follows it.
	 */
	function ask(season: string) {
		// The newest season needs no name: the address of the page alone is
		// this year's ranking next year too.
		const named = season === ranking.seasons[0] ? '' : season;
		void goto(rankingHref(named), { replaceState: true, keepFocus: true, noScroll: true });
	}

	/** A wide screen has a table; a phone, the list of cards. */
	const wide = new MediaQuery('min-width: 56rem', false);
</script>

<svelte:head>
	<title>Ranking · Venti</title>
</svelte:head>

<div class="desk page">
	<header class="page-head">
		<h1>Logbook</h1>
		<!-- On a wide screen the season is chosen in the page's head. -->
		{#if wide.current && !app.failure && ranking.seasons.length > 0}
			<div class="actions season-head">
				<Segmented
					label="Season"
					options={ranking.seasons}
					bind:value={() => ranking.season, (chosen) => ask(chosen)}
				/>
			</div>
		{/if}
	</header>
	<div class="whose">
		<WhoseFlights current="ranking" query={everyone ? queryOf(everyone.asked) : ''} />
	</div>

	{#if app.failure}
		<div class="nothing">
			<span class="symbol"><Icon name="warning" /></span>
			<h2>Venti could not open its data</h2>
			<p>{app.failure}</p>
		</div>
	{:else}
		{#if ranking.seasons.length > 0 && !wide.current}
			<div class="season">
				<Segmented
					label="Season"
					options={ranking.seasons}
					bind:value={() => ranking.season, (chosen) => ask(chosen)}
				/>
			</div>
		{/if}

		{#if ranking.pilots.length > 0}
			{#if wide.current}
				<div class="table">
					<RankingTable label="Ranking of {ranking.season}" pilots={ranking.pilots} />
				</div>
			{:else}
				<RankingList label="Ranking of {ranking.season}" pilots={ranking.pilots} />
			{/if}
			<p class="footnote">{ranking.rule}</p>
		{:else if ranking.loading || (!ranking.known && !ranking.problem && !ranking.absent)}
			<p class="waiting" role="status">Fetching the ranking…</p>
		{:else}
			<div class="nothing">
				<span class="symbol"><Icon name={ranking.problem ? 'warning' : 'person'} /></span>
				<h2>{nothing.title}</h2>
				<p>{nothing.says}</p>
				{#if ranking.problem}
					<button class="button-plain" type="button" onclick={() => everyone?.rankAgain()}>
						Try again
					</button>
				{/if}
			</div>
		{/if}
	{/if}
</div>

<style>
	/* The `.desk`'s column, of the whole width on a wide screen. */
	.page {
		display: flex;
		flex-direction: column;
		min-height: calc(100dvh - var(--nav-top));
	}

	header {
		min-height: 3.25rem;
		margin-bottom: 0.75rem;
	}

	.actions.season-head {
		display: block;
		width: 30rem;
	}

	.whose {
		margin-bottom: 1rem;
	}

	h1 {
		font-size: var(--large-title);
		font-weight: 600;
	}

	.season {
		max-width: 28rem;
		margin-bottom: 1rem;
	}

	.footnote {
		margin-top: 0.75rem;
		padding: 0 1rem;
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	.waiting {
		padding: 3rem 1rem;
		text-align: center;
		color: var(--text-secondary);
	}

	.nothing {
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

	.nothing h2 {
		font-size: var(--title);
		font-weight: 600;
	}

	.nothing p {
		color: var(--text-secondary);
	}

	.nothing button {
		margin-top: 0.5rem;
	}

	@media (min-width: 56rem) {
		header {
			min-height: 0;
		}
	}

	.table {
		overflow-x: auto;
		background: var(--surface);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-m);
	}
</style>
