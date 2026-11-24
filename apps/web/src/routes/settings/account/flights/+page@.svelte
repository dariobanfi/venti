<!--
	My flights: the pilot's flights on the Venti account, by day, the newest
	first. The website keeps no logbook of its own (owner decision 20): a
	pilot who is not signed in is asked to, and importing a file is an
	upload to the account.

	It is the account's page, reached from the account and from Flights by
	its "Mine", and drawn whole-width as the other pages of flights are, out
	of the settings' frame.
-->
<script lang="ts">
	import { getApp } from '#lib/app/app.svelte.ts';
	import { accountHref, myStatisticsHref, pilotHref } from '#lib/app/routes.ts';
	import { queryOf } from '#lib/community/pages.ts';
	import WhoseFlights from '#lib/community/WhoseFlights.svelte';
	import type { OwnFlight } from '#lib/core/index.ts';
	import { flightFormat, format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import MyFlightRow from '#lib/logbook/MyFlightRow.svelte';

	const app = getApp();
	const flights = $derived(app.myFlights);
	const text = format();

	let picker = $state<HTMLInputElement>();

	/** The files the pilot chose. The core and the server say which of them are flights. */
	function picked() {
		if (!picker) return;
		const files = [...(picker.files ?? [])];
		// Cleared so that choosing the same file again is noticed.
		picker.value = '';
		void app.importFiles(files);
	}

	/** The flights by the day they began on, by the clock of the place each was flown, the newest first. */
	const days = $derived.by(() => {
		const grouped: { day: string; flights: OwnFlight[]; airtimeS: number }[] = [];
		for (const flight of flights?.flights ?? []) {
			const day = flightFormat(flight.row).date(flight.row.startUnixS);
			const last = grouped.at(-1);
			if (last?.day === day) {
				last.flights.push(flight);
				last.airtimeS += flight.row.airtimeS;
			} else {
				grouped.push({ day, flights: [flight], airtimeS: flight.row.airtimeS });
			}
		}
		return grouped;
	});

	const handle = $derived(app.account?.signedIn ? app.account.account?.handle : undefined);
</script>

<svelte:head>
	<title>My flights · Venti</title>
</svelte:head>

<div class="page">
	<header>
		<h1>My flights</h1>
		{#if flights?.phase.is === 'loaded'}
			<button class="import" type="button" onclick={() => picker?.click()}>
				<Icon name="plus" />
				<span>Import flights</span>
			</button>
		{/if}
	</header>

	{#if app.account?.hasServer}
		<div class="whose">
			<WhoseFlights current="mine" query={app.everyone ? queryOf(app.everyone.asked) : ''} />
		</div>
	{/if}

	{#if flights?.progress}
		<p class="status" role="status">
			Importing {flights.progress.done} of {flights.progress.of}
		</p>
	{/if}

	{#if app.failure}
		<div class="unavailable">
			<span class="symbol"><Icon name="warning" /></span>
			<h2>Venti could not open</h2>
			<p>{app.failure}</p>
		</div>
	{:else if !flights || flights.phase.is === 'loading'}
		<p class="status" role="status">Fetching your flights…</p>
	{:else if flights.phase.is === 'signedOut'}
		<div class="unavailable">
			<span class="symbol"><Icon name="person" /></span>
			<h2>Sign in to see your flights</h2>
			<div class="actions">
				<a class="button-prominent" href={accountHref()}>{app.said?.common.signIn}</a>
			</div>
		</div>
	{:else if flights.phase.is === 'failed'}
		<div class="unavailable">
			<span class="symbol"><Icon name="warning" /></span>
			<h2>Your flights could not be fetched</h2>
			<p>{flights.phase.message}</p>
			<div class="actions">
				<button class="button-plain" type="button" onclick={() => flights.reload()}>
					{app.said?.common.tryAgain}
				</button>
			</div>
		</div>
	{:else if days.length === 0}
		<div class="unavailable">
			<span class="symbol"><Icon name="book" /></span>
			<h2>No flights yet</h2>
			<p>
				Import the IGC files your vario or flight app recorded. They stay on your account.
				<span class="with-mouse">Or drag them onto this page.</span>
			</p>
			<div class="actions">
				<button class="button-prominent" type="button" onclick={() => picker?.click()}>
					Import flights
				</button>
			</div>
		</div>
	{:else}
		<p class="totals">
			<a href={myStatisticsHref()}>Statistics</a>
			{#if handle}
				<a href={pilotHref(handle)}>Your public page</a>
			{/if}
		</p>
		{#each days as day (day.day)}
			<section class="day" aria-labelledby="day-{day.flights[0].row.id}">
				<h2 id="day-{day.flights[0].row.id}">
					<span>{day.day}</span>
					{#if day.flights.length > 1}
						<span class="day-total numeric">
							{text.flights(day.flights.length)} · {text.hoursMinutes(day.airtimeS)} h
						</span>
					{/if}
				</h2>
				<ul class="card" aria-label={day.day}>
					{#each day.flights as flight (flight.row.id)}
						<MyFlightRow {flight} />
					{/each}
				</ul>
			</section>
		{/each}
		{#if flights.next}
			<div class="more">
				<button
					class="button-plain"
					type="button"
					disabled={flights.loadingMore}
					onclick={() => flights.more()}
				>
					{flights.loadingMore ? 'Fetching…' : 'Show older flights'}
				</button>
			</div>
		{/if}
	{/if}
</div>

<!-- Flight files have no type a browser knows, so any file can be picked. -->
<input
	bind:this={picker}
	class="visually-hidden"
	type="file"
	multiple
	tabindex="-1"
	aria-hidden="true"
	onchange={picked}
/>

<style>
	.page {
		display: flex;
		flex-direction: column;
		min-height: calc(100dvh - var(--nav-top));
		max-width: 60rem;
		margin: 0 auto;
		padding: calc(1rem + var(--safe-top)) calc(var(--gutter) + var(--safe-right))
			calc(1.5rem + var(--nav-bottom)) calc(var(--gutter) + var(--safe-left));
	}

	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		min-height: 3.25rem;
		margin-bottom: 0.75rem;
	}

	h1 {
		font-size: var(--large-title);
		font-weight: 600;
	}

	.whose {
		margin-bottom: 1rem;
	}

	/* Narrow: a round button with a plus, as in the iPhone's toolbar. */
	.import {
		display: grid;
		place-items: center;
		width: var(--target);
		height: var(--target);
		border-radius: 50%;
		background: var(--surface);
		color: var(--link);
		box-shadow: var(--shadow-float);
	}

	.import span {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip-path: inset(50%);
		white-space: nowrap;
	}

	.status {
		margin-bottom: 0.75rem;
		font-size: var(--subheadline);
		color: var(--text-secondary);
	}

	.totals {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2) var(--space-4);
		margin-bottom: var(--space-2);
	}

	.totals a {
		color: var(--link);
	}

	/* A day: its heading, then its flights in one card. */
	.day {
		margin-bottom: var(--space-5);
	}

	.day h2 {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: var(--space-3);
		margin: 0 0 var(--space-2) var(--space-1);
		font-size: var(--body);
		font-weight: 600;
	}

	.day-total {
		font-size: var(--subheadline);
		font-weight: 500;
		color: var(--text-secondary);
	}

	.card {
		margin: 0;
		padding: 0;
		list-style: none;
		border-radius: var(--radius-card);
		background: var(--surface);
		overflow: clip;
	}

	.more {
		display: flex;
		justify-content: center;
		margin-top: var(--space-2);
	}

	/* A screen with nothing on it says so, and what to do next. */
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

	.with-mouse {
		display: none;
	}

	.actions {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.5rem;
		margin-top: 1rem;
	}

	@media (hover: hover) and (pointer: fine) {
		.with-mouse {
			display: inline;
		}
	}

	@media (min-width: 56rem) {
		.page {
			max-width: 80rem;
			padding-top: var(--space-4);
		}

		header {
			min-height: 0;
			margin-bottom: var(--space-2);
		}

		.whose {
			margin-bottom: var(--space-3);
		}

		/* Wide: the button has room to say what it does. */
		.import {
			display: inline-flex;
			gap: 0.375rem;
			width: auto;
			height: var(--target);
			padding: 0 var(--space-3) 0 var(--space-2);
			border-radius: var(--radius-s);
			background: var(--accent);
			color: var(--on-accent);
			font-weight: 600;
			box-shadow: none;
		}

		.import span {
			position: static;
			width: auto;
			height: auto;
			clip-path: none;
		}
	}
</style>
