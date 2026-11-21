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
	import { NOT_SEARCHED } from '#lib/community/search.ts';
	import { nothingListed, RECKONING } from '#lib/community/words.ts';
	import Icon from '#lib/design/Icon.svelte';
	import Segmented from '#lib/design/Segmented.svelte';
	import { MediaQuery } from 'svelte/reactivity';

	const app = getApp();
	const everyone = $derived(app.everyone);

	/** What the address asks for. It is not known while the pages are written out at build time. */
	const asked = $derived(browser ? askedIn(page.url.searchParams) : EVERYTHING);
	const list = $derived(everyone?.list ?? NOT_LISTED);

	$effect(() => everyone?.show(asked));
	// The head of the list: the best of today and the top of the year.
	$effect(() => everyone?.showFront(dayOf(new Date())));
	/** Whether the list is the plain one, newest first on any day, which the head stands over. */
	const plain = $derived(asked.order === 'newest' && !asked.from && !asked.to);

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

	/** Whether the side column stands beside the table. */
	const beside = $derived(wide.current && plain && Boolean(everyone?.front));

	const id = $props.id();
</script>

<svelte:head>
	<title>Everyone’s flights · Venti</title>
</svelte:head>

<div class="desk page">
	<header>
		<h1>Logbook</h1>
	</header>
	<div class="whose">
		<WhoseFlights current="everyone" query={queryOf(asked)} />
	</div>

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
			<div class="order">
				<Segmented
					label="Order"
					options={ORDERS}
					bind:value={
						() => (asked.order === 'best' ? 'Best' : 'Newest'),
						(chosen) => ask({ ...asked, order: chosen === 'Best' ? 'best' : 'newest' })
					}
				/>
			</div>
			<!-- A day, or a span of days: choosing the first alone means that day. -->
			<div class="days" role="group" aria-label="Days">
				<span class="day">
					<label for="{id}-from">From</label>
					<input
						id="{id}-from"
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
						type="date"
						value={asked.to}
						onchange={(event) =>
							ask({ ...asked, ...withDay(asked, 'to', event.currentTarget.value) })}
					/>
				</span>
				{#if asked.from || asked.to}
					<button
						class="button-plain"
						type="button"
						onclick={() => ask({ ...asked, from: '', to: '' })}
					>
						Any day
					</button>
				{/if}
			</div>
			<div class="who">
				<span class="day">
					<label for="{id}-who">Pilot</label>
					<input
						id="{id}-who"
						type="search"
						placeholder="Name or handle"
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
								<li><a href={pilotHref(pilot.handle)}>{pilot.words}</a></li>
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
				<PublicFlightList
					label="Everyone’s flights"
					{list}
					named
					nothing={nothingListed(list, asked)}
					reckoning={RECKONING}
					onmore={() => everyone?.more()}
					onagain={() => everyone?.again()}
				/>
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

	header {
		display: flex;
		align-items: center;
		min-height: 3.25rem;
		margin-bottom: 0.75rem;
	}

	.whose {
		margin-bottom: 1rem;
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

	input {
		/* Room for a date as the browser writes it, and both fields in one
		   row of a phone. */
		width: 8.5rem;
		min-height: 2.25rem;
		padding: 0 0.5rem;
		border: 0;
		border-radius: 0.625rem;
		background: var(--fill);
		color: var(--text);
		font: inherit;
		font-variant-numeric: tabular-nums;
	}

	.days button {
		min-height: 2.25rem;
		padding: 0 0.625rem;
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
		header {
			min-height: 0;
			margin-bottom: var(--space-3);
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

		.days button {
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
