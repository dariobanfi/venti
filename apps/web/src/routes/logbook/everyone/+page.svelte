<!--
	Everyone's flights: what the pilots of this site's Venti server share with
	everyone, the newest first or the best by points, on any day or on the
	days chosen. What is asked for is in the address, so the list can be come
	back to and handed on: `?order=best&from=2026-08-15&to=2026-08-16`.
-->
<script lang="ts">
	import { browser } from '$app/env';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { everyoneHref } from '#lib/app/routes.ts';
	import {
		type Asked,
		askedIn,
		EVERYTHING,
		NOT_LISTED,
		queryOf,
		withDay
	} from '#lib/community/pages.ts';
	import PublicFlightList from '#lib/community/PublicFlightList.svelte';
	import WhoseFlights from '#lib/community/WhoseFlights.svelte';
	import { nothingListed, RECKONING } from '#lib/community/words.ts';
	import Icon from '#lib/design/Icon.svelte';
	import Segmented from '#lib/design/Segmented.svelte';

	const app = getApp();
	const everyone = $derived(app.everyone);

	/** What the address asks for. It is not known while the pages are written out at build time. */
	const asked = $derived(browser ? askedIn(page.url.searchParams) : EVERYTHING);
	const list = $derived(everyone?.list ?? NOT_LISTED);

	$effect(() => everyone?.show(asked));

	/**
	 * Asks for something else: the address changes in place, with the
	 * keyboard and the scroll where they are, and the list follows it.
	 */
	function ask(next: Asked) {
		void goto(everyoneHref(queryOf(next)), { replace: true, reset: false });
	}

	const ORDERS = ['Newest', 'Best'] as const;

	const id = $props.id();
</script>

<svelte:head>
	<title>Everyone’s flights · Venti</title>
</svelte:head>

<div class="page">
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
		</div>
	{/if}

	{#if !app.failure}
		<PublicFlightList
			label="Everyone’s flights"
			{list}
			named
			nothing={nothingListed(list, asked)}
			reckoning={RECKONING}
			onmore={() => everyone?.more()}
			onagain={() => everyone?.again()}
		/>
	{/if}
</div>

<style>
	.page {
		display: flex;
		flex-direction: column;
		min-height: 100dvh;
		max-width: 60rem;
		margin: 0 auto;
		padding: calc(1rem + var(--safe-top)) calc(var(--gutter) + var(--safe-right))
			calc(1.5rem + var(--nav-bottom)) calc(var(--gutter) + var(--safe-left));
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
		font-weight: 700;
		letter-spacing: -0.01em;
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
		font-weight: 700;
	}

	.unavailable p {
		color: var(--text-secondary);
	}

	@media (min-width: 56rem) {
		.page {
			padding-top: 2rem;
		}
	}
</style>
