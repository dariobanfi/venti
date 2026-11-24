<!--
	Everyone's flights: what the pilots of this site's Venti server share with
	everyone, the newest first or the best by points, on any day or on the
	days chosen. What is asked for is in the address, so the list can be come
	back to and handed on: `?order=best&from=2026-08-15&to=2026-08-16`.

	A wide screen has the whole width: what is asked in one row, the table,
	and beside it the best of today and the top ten of the season.

	The box for a pilot's name asks the server once the typing rests, and
	lists under itself the core's line for each pilot found, which opens
	that pilot's page; the core's line where nobody is found, and its
	words for a text too short to ask for.
-->
<script lang="ts">
	import PilotFace from '#lib/community/PilotFace.svelte';
	import { browser } from '$app/env';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { everyoneHref, pilotHref } from '#lib/app/routes.ts';
	import {
		type Asked,
		askedIn,
		EVERYTHING,
		NOT_LISTED,
		queryOf,
		withDay
	} from '#lib/community/pages.ts';
	import FrontHead from '#lib/community/FrontHead.svelte';
	import PublicFlightList from '#lib/community/PublicFlightList.svelte';
	import { dayOf } from '#lib/groups/days.ts';
	import WhoseFlights from '#lib/community/WhoseFlights.svelte';
	import SearchFilter from '#lib/community/SearchFilter.svelte';
	import { isSearching, NO_SEARCH } from '#lib/community/flight-search.ts';
	import { NOT_SEARCHED } from '#lib/community/search.ts';
	import { nothingListed, RECKONING } from '#lib/community/words.ts';
	import { format } from '#lib/design/format.ts';
	import ChipTick from '#lib/design/ChipTick.svelte';
	import Icon from '#lib/design/Icon.svelte';
	import Segmented from '#lib/design/Segmented.svelte';
	import { feedAsList, NO_FEED, saidOf } from '#lib/social/model.svelte.ts';
	import { MediaQuery } from 'svelte/reactivity';

	const app = getApp();
	const text = format();
	const everyone = $derived(app.everyone);

	/** What the address asks for. It is not known while the pages are written out at build time. */
	const asked = $derived(browser ? askedIn(page.url.searchParams) : EVERYTHING);
	const list = $derived(everyone?.list ?? NOT_LISTED);

	$effect(() => everyone?.show(asked));

	/*
	 * "Following", beside "Everyone": the flights of the pilots one follows,
	 * for a signed-in pilot. The address says which (`?feed=following`).
	 */
	const social = $derived(app.social);
	const said = $derived(social?.words);
	const own = $derived(app.account?.signedIn ? app.account.account?.handle : undefined);
	const following = $derived(
		browser && Boolean(own) && page.url.searchParams.get('feed') === 'following'
	);
	$effect(() => {
		if (following) social?.showFeed(own);
	});
	const feed = $derived(social?.feed ?? NO_FEED);

	function choose(whose: string) {
		const query =
			whose === said?.feedFollowing ? '?feed=following' : queryOf({ ...asked, order: 'newest' });
		void goto(everyoneHref(query), { replaceState: true, keepFocus: true, noScroll: true });
	}
	// The head of the list: the best of today and the top of the year.
	$effect(() => everyone?.showFront(dayOf(new Date())));
	/** Whether the list is the plain one, newest first on any day, which the head stands over. */
	const plain = $derived(
		asked.order === 'newest' && !asked.from && !asked.to && !isSearching(asked.search) && !following
	);

	/**
	 * Asks for something else: the address changes in place, with the
	 * keyboard and the scroll where they are, and the list follows it.
	 */
	function ask(next: Asked) {
		void goto(everyoneHref(queryOf(next)), { replaceState: true, keepFocus: true, noScroll: true });
	}

	const ORDERS = ['Newest', 'Best'] as const;

	/** A wide screen has the head of the list in a column beside it. */
	const wide = new MediaQuery('min-width: 56rem', false);

	/** The name, or part of it, of the pilot looked for on the server. */
	let who = $state('');
	$effect(() => everyone?.findPilots(who));
	const search = $derived(everyone?.search ?? NOT_SEARCHED);
	/** What the field says under itself: nothing while it is empty or the list is put away. */
	let open = $state(true);
	const answered = $derived(open && search.text !== '' && !search.waiting);

	/** Whether the pickers of the days are unfolded. */
	let filtering = $state(false);
	/** The day or the span of days chosen, as the chip beside "Filter" says it. */
	const span = $derived.by(() => {
		const named = (day: string) => {
			const parts = day.split('-').map(Number);
			// Noon by UTC is the same date wherever the reader is.
			return parts.length === 3 && parts.every(Number.isFinite)
				? text.date(Date.UTC(parts[0], parts[1] - 1, parts[2], 12) / 1000)
				: day;
		};
		const { from, to } = asked;
		if (!from && !to) return '';
		if (from && to) return from === to ? named(from) : `${named(from)} – ${named(to)}`;
		return from ? `From ${named(from)}` : `Until ${named(to)}`;
	});

	/** Whether the side column stands beside the table. */
	const beside = $derived(wide.current && plain && Boolean(everyone?.front));

	const id = $props.id();
</script>

<svelte:head>
	<title>{app.said ? `${app.said.pages.everyonesFlights} · Venti` : 'Venti'}</title>
</svelte:head>

<div class="desk page">
	<!-- The switch between the pilot's own flights ("Mine"), which are the
	     account's, and everyone's, first on the page at every width. -->
	<header>
		<h1>Flights</h1>
		<WhoseFlights current="everyone" query={queryOf(asked)} />
	</header>

	{#if app.failure}
		<div class="unavailable">
			<span class="symbol"><Icon name="warning" /></span>
			<h2>Venti could not open its data</h2>
			<p>{app.failure}</p>
		</div>
	{:else if !list.absent}
		{#if plain && everyone?.front && !wide.current}
			<FrontHead front={everyone.front} toEveryone={false} />
		{/if}
		<div class="asked">
			{#if own && said}
				<div class="order">
					<Segmented
						label="Whose flights"
						options={[said.feedEveryone, said.feedFollowing]}
						bind:value={
							() => (following ? said.feedFollowing : said.feedEveryone), (chosen) => choose(chosen)
						}
					/>
				</div>
			{/if}
			<div class="order" hidden={following}>
				<Segmented
					label="Order"
					options={ORDERS}
					bind:value={
						() => (asked.order === 'best' ? 'Best' : 'Newest'),
						(chosen) => ask({ ...asked, order: chosen === 'Best' ? 'best' : 'newest' })
					}
				/>
			</div>
			<!-- The days and the search are behind one control; each choice
			     stands beside it as a chip, which also lets go of it. -->
			{#if !following}
				<SearchFilter
					search={asked.search ?? NO_SEARCH}
					onsearch={(search) => ask({ ...asked, search })}
					bind:open={filtering}
				>
					{#snippet chips()}
						{#if span}
							<span class="span numeric">
								<ChipTick on />
								{span}
								<button
									type="button"
									aria-label={app.said?.pages.anyDay}
									title={app.said?.pages.anyDay}
									onclick={() => ask({ ...asked, from: '', to: '' })}
								>
									<Icon name="close" />
								</button>
							</span>
						{/if}
					{/snippet}
					{#snippet fields()}
						<!-- A day, or a span of days: choosing the first alone means that day. -->
						<div class="days" role="group" aria-label="Days">
							<span class="day">
								<label for="{id}-from">From</label>
								<input
									id="{id}-from"
									class="text-field"
									type="date"
									value={asked.from}
									onchange={(event) =>
										ask({ ...asked, ...withDay(asked, 'from', event.currentTarget.value) })}
								/>
							</span>
							<span class="day">
								<label for="{id}-to">to</label>
								<input
									id="{id}-to"
									class="text-field"
									type="date"
									value={asked.to}
									onchange={(event) =>
										ask({ ...asked, ...withDay(asked, 'to', event.currentTarget.value) })}
								/>
							</span>
						</div>
					{/snippet}
				</SearchFilter>
			{/if}
			<div class="who">
				<span class="day">
					<label for="{id}-who">Pilot</label>
					<input
						id="{id}-who"
						class="text-field"
						type="search"
						placeholder={everyone?.pilotsWords?.searchPlaceholder ?? ''}
						autocomplete="off"
						aria-controls="{id}-found"
						bind:value={who}
						oninput={() => (open = true)}
						onfocus={() => (open = true)}
						onkeydown={(event) => {
							if (event.key === 'Escape') open = false;
						}}
					/>
				</span>
				<div id="{id}-found" class="found" class:shown={answered}>
					<p class="result" role="status">
						{#if answered}{search.problem ?? search.found?.words ?? ''}{/if}
					</p>
					{#if answered && search.found && search.found.pilots.length > 0}
						<ul aria-label="Pilots found">
							{#each search.found.pilots as pilot (pilot.handle)}
								<li>
									<a href={pilotHref(pilot.handle)}>
										<PilotFace picture={pilot.picturePath} name={pilot.name} />
										<span>{pilot.words}</span>
									</a>
								</li>
							{/each}
						</ul>
					{/if}
				</div>
			</div>
		</div>
	{/if}

	{#if !app.failure}
		<div class="columns">
			<div class={beside ? 'span-9' : 'span-12'}>
				{#if following && said}
					<PublicFlightList
						label={said.feedFollowing}
						list={feedAsList(feed)}
						named
						nothing={feed.problem
							? { title: said.feedFailed, says: feed.problem }
							: { title: said.feedFollowing, says: feed.emptyLine }}
						reckoning={RECKONING}
						said={{
							cheersTitle: said.cheersTitle,
							commentsTitle: said.commentsTitle,
							of: saidOf(feed.rows)
						}}
						onmore={() => social?.moreFeed()}
						onagain={() => social?.againFeed()}
					/>
				{:else}
					<PublicFlightList
						label={app.said?.pages.everyonesFlights ?? ''}
						{list}
						named
						nothing={nothingListed(list, asked)}
						reckoning={RECKONING}
						onmore={() => everyone?.more()}
						onagain={() => everyone?.again()}
					/>
				{/if}
			</div>
			{#if beside && everyone?.front}
				<aside class="span-3" aria-label="Best of today and the top of the season">
					<FrontHead front={everyone.front} toEveryone={false} ranked={10} />
				</aside>
			{/if}
		</div>
	{/if}
</div>

<style>
	/* The `.desk`'s column, of the whole width on a wide screen. */
	.page {
		display: flex;
		flex-direction: column;
		min-height: calc(100dvh - var(--nav-top));
	}

	/* The best of today and the season beside the table where both fit,
	   under it where they do not. */
	@media (max-width: 79.99rem) {
		.columns > .span-9,
		.columns > .span-3 {
			grid-column: 1 / -1;
		}
	}

	header {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		margin-bottom: var(--space-4);
	}

	h1 {
		font-size: var(--large-title);
		font-weight: 600;
	}

	/* What is asked of the list: its order, and the days. */
	.asked {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: 0.75rem 1.5rem;
		margin-bottom: 1rem;
	}

	.order {
		flex: 1 1 12rem;
		max-width: 16rem;
	}

	/* The pack's chip, chosen: what the list is held to. */
	.span {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		min-height: var(--space-6);
		padding: 0 var(--space-1) 0 var(--space-3);
		border-radius: var(--radius-pill);
		background: var(--accent);
		color: var(--on-accent);
		font-size: var(--subheadline);
		font-weight: 500;
	}

	.span button {
		display: grid;
		place-items: center;
		width: 1.75rem;
		height: 1.75rem;
		border-radius: 50%;
		color: inherit;
	}

	.span :global(.tick) {
		margin-inline-end: 0;
	}

	.span button :global(svg) {
		width: 1rem;
		height: 1rem;
	}

	[hidden] {
		display: none;
	}

	.days {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.375rem 0.5rem;
		font-size: var(--subheadline);
	}

	/* A field and its name stay together when the row wraps. */
	.day {
		display: inline-flex;
		align-items: center;
		gap: 0.5rem;
	}

	.days label {
		color: var(--text-secondary);
	}

	/* The pack's field (`.text-field`), narrower: room for a date as the
	   browser writes it, and both fields in one row of a phone. */
	input {
		width: 8.5rem;
		min-height: 2.25rem;
		padding: 0 0.5rem;
		font-variant-numeric: tabular-nums;
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
		font-size: var(--title);
		font-weight: 600;
	}

	.unavailable p {
		color: var(--text-secondary);
	}

	/* The search for a pilot: the field, and under it what was found, over
	   the list. */
	.who {
		position: relative;
	}

	.who input {
		width: 12rem;
	}

	.found {
		display: none;
		position: absolute;
		top: calc(100% + var(--space-1));
		left: 0;
		z-index: 2;
		width: max-content;
		min-width: 100%;
		max-width: min(22rem, calc(100vw - 2 * var(--space-4)));
		border: 1px solid var(--color-border);
		border-radius: var(--radius-m);
		background: var(--surface);
		box-shadow: var(--shadow-float);
		overflow: hidden;
	}

	.found.shown {
		display: block;
	}

	.result {
		padding: var(--space-2) var(--space-3);
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	.found ul {
		list-style: none;
	}

	.found a {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		min-height: var(--target);
		padding: var(--space-2) var(--space-3);
		box-shadow: inset 0 0.5px 0 var(--separator);
		color: var(--link);
		overflow-wrap: anywhere;
	}

	@media (hover: hover) {
		.found a:hover {
			background: var(--fill);
		}
	}

	.found a:focus-visible {
		outline-offset: -0.1875rem;
	}

	/* Wide: what is asked in one row, at the controls' height of a pointer. */
	@media (min-width: 56rem) {
		/* The switch beside the title. */
		header {
			flex-direction: row;
			align-items: center;
			gap: var(--space-6);
		}

		.asked {
			justify-content: flex-start;
			gap: var(--space-3) var(--space-6);
			padding: var(--space-2) var(--space-3);
			border: 1px solid var(--color-border);
			border-radius: var(--radius-m);
			background: var(--surface);
		}

		.order {
			flex: none;
			width: 12rem;
		}

		input {
			min-height: var(--target);
		}

		.who {
			margin-left: auto;
		}

		.who input {
			width: 14rem;
		}

		/* At the row's right end the list opens leftwards. */
		.found {
			left: auto;
			right: 0;
		}
	}
</style>
