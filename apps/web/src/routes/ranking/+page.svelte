<!--
	The ranking: the pilots of this site's Venti server by the sum of the
	points of their best flights that they share with everyone, in a year or
	in all years, or in one month of a year. Which is in the address, so the
	page can be come back to and handed on: `?season=2026`, `?month=2026-05`.

	Under the years stand the months of the chosen year that have a ranking,
	as the core names them (`Ranking.periods`): a month is pressed to see
	its ranking and pressed again for the whole year. The line over the list
	says the period in the core's words.
-->
<script lang="ts">
	import { browser } from '$app/env';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { rankingHref } from '#lib/app/routes.ts';
	import { NOT_RANKED } from '#lib/community/model.svelte.ts';
	import RankingList from '#lib/community/RankingList.svelte';
	import RankingTable from '#lib/community/RankingTable.svelte';
	import { MediaQuery } from 'svelte/reactivity';
	import { nothingRanked } from '#lib/community/words.ts';
	import ChipTick from '#lib/design/ChipTick.svelte';
	import Icon from '#lib/design/Icon.svelte';
	import YearChips from '#lib/design/YearChips.svelte';

	const app = getApp();
	const everyone = $derived(app.everyone);

	/** The season the address asks for. It is not known while the pages are written out at build time. */
	const asked = $derived(browser ? (page.url.searchParams.get('season') ?? '') : '');
	/** The month the address asks for, as the ranking's periods write one. */
	const askedMonth = $derived(browser ? (page.url.searchParams.get('month') ?? '') : '');
	const ranking = $derived(everyone?.ranking ?? NOT_RANKED);
	/** The core's words around the ranking: the season's choice, the empty month's sentence. */
	const said = $derived(everyone?.pilotsWords);
	const nothing = $derived(nothingRanked({ ...ranking, monthEmpty: said?.monthEmpty }));

	$effect(() => everyone?.showRanking(asked, askedMonth));

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

	/** Asks for a month of the season, or, for the month that is shown, for the whole season again. */
	function askMonth(month: string) {
		const whole = ranking.season === ranking.seasons[0] ? '' : ranking.season;
		const href = month === ranking.askedMonth ? rankingHref(whole) : rankingHref('', month);
		void goto(href, { replaceState: true, keepFocus: true, noScroll: true });
	}

	/** What the ranking is of, as the core calls it. */
	const title = $derived(ranking.title || ranking.season);

	/** A wide screen has a table; a phone, the list of cards. */
	const wide = new MediaQuery('min-width: 56rem', false);
</script>

<svelte:head>
	<title>{ranking.month ? `Ranking of ${ranking.month.title}` : 'Ranking'} · Venti</title>
</svelte:head>

<div class="desk page">
	<header class="page-head">
		<h1>Ranking</h1>
	</header>

	{#if app.failure}
		<div class="nothing">
			<span class="symbol"><Icon name="warning" /></span>
			<h2>Venti could not open its data</h2>
			<p>{app.failure}</p>
		</div>
	{:else}
		{#if ranking.seasons.length > 0}
			<div class="season">
				<YearChips
					label={said?.seasonChoice ?? ''}
					options={ranking.seasons}
					value={ranking.season}
					onchoose={ask}
				/>
			</div>
		{/if}

		{#if ranking.months.length > 0}
			<div class="months" role="group" aria-label="Months of {ranking.season}">
				<!-- The whole year first, in the core's words: "All of 2026". -->
				{#if ranking.wholeName}
					<button
						class="month"
						type="button"
						aria-pressed={ranking.askedMonth === ''}
						onclick={() => {
							if (ranking.askedMonth) askMonth(ranking.askedMonth);
						}}
					>
						<ChipTick on={ranking.askedMonth === ''} />
						{ranking.wholeName}
					</button>
				{/if}
				{#each ranking.months as month (month.month)}
					<button
						class="month"
						type="button"
						aria-pressed={month.month === ranking.askedMonth}
						aria-label={month.words}
						onclick={() => askMonth(month.month)}
					>
						<ChipTick on={month.month === ranking.askedMonth} />
						{month.name}
					</button>
				{/each}
			</div>
		{/if}

		{#if ranking.pilots.length > 0}
			<!-- The ranking's heading is the period's title, as the core has it. -->
			{#if title}
				<h2 class="period-title">{title}</h2>
			{/if}
			{#if ranking.words}
				<p class="period">{ranking.words}</p>
			{/if}
			{#if wide.current}
				<div class="table">
					<RankingTable label="Ranking of {title}" pilots={ranking.pilots} />
				</div>
			{:else}
				<RankingList label="Ranking of {title}" pilots={ranking.pilots} />
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
						{app.said?.common.tryAgain}
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

	h1 {
		font-size: var(--large-title);
		font-weight: 600;
	}

	.season {
		margin-bottom: var(--space-3);
	}

	.period-title {
		margin-bottom: var(--space-1);
		padding: 0 var(--space-4);
		font-size: var(--title);
		font-weight: 600;
	}

	/* The months of the season that have a ranking: the pack's chips. */
	.months {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
		margin-bottom: var(--space-3);
	}

	.month {
		min-height: calc(var(--target) - 0.625rem);
		padding: 0 var(--space-3);
		border-radius: var(--radius-pill);
		background: var(--fill);
		color: var(--text-secondary);
		font-size: var(--subheadline);
		font-weight: 500;
	}

	.month[aria-pressed='true'] {
		background: var(--accent);
		color: var(--on-accent);
	}

	/* The period in the core's words, over the list. */
	.period {
		margin-bottom: var(--space-2);
		padding: 0 var(--space-4);
		font-size: var(--subheadline);
		color: var(--text-secondary);
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
