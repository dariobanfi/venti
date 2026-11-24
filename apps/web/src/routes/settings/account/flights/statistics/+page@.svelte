<!--
	What all the pilot's flights add up to, the private ones too: the
	account's statistics as the server adds them up and the core reads them
	(`ownStatisticsRequest`, `ownStatistics`, whose note under them says
	that the private flights count too), shown as a pilot's public totals
	are (`PilotTotals.svelte`). Every word and number is the core's.

	It belongs to My flights and is drawn whole-width as it is, out of the
	settings' frame. A row about one flight opens it among the pilot's own.
	At its foot, "How you fly" (`how-you-fly/`).
-->
<script lang="ts">
	import { untrack } from 'svelte';
	import { getApp } from '#lib/app/app.svelte.ts';
	import {
		accountHref,
		howYouFlyHref,
		myFlightHref,
		myFlightsHref,
		pilotHref
	} from '#lib/app/routes.ts';
	import PilotTotals from '#lib/community/PilotTotals.svelte';
	import type { PilotStatistics } from '#lib/core/index.ts';
	import Icon from '#lib/design/Icon.svelte';
	import { reasonFor } from '#lib/logbook/model.svelte.ts';

	const app = getApp();

	type Phase =
		| { is: 'loading' }
		| { is: 'loaded'; statistics: PilotStatistics }
		| { is: 'signedOut' }
		| { is: 'failed'; message: string };

	let phase = $state.raw<Phase>({ is: 'loading' });
	/** Counts the pilot's "Try again". */
	let again = $state(0);

	$effect(() => {
		void again;
		if (app.failure) {
			phase = { is: 'failed', message: app.failure };
			return;
		}
		const flights = app.myFlights;
		if (!flights) return;
		// Asked again when the pilot signs in or out, and after a change to the flights.
		void flights.revision;
		if (!app.account?.signedIn) {
			phase = { is: 'signedOut' };
			return;
		}
		// Only the answer to the latest question counts.
		let current = true;
		if (untrack(() => phase.is) !== 'loaded') phase = { is: 'loading' };
		flights.statistics().then(
			(statistics) => {
				if (current) phase = { is: 'loaded', statistics };
			},
			(error) => {
				if (current) phase = { is: 'failed', message: reasonFor(error) };
			}
		);
		return () => {
			current = false;
		};
	});

	const handle = $derived(app.account?.signedIn ? app.account.account?.handle : undefined);
</script>

<svelte:head>
	<title>Statistics · Venti</title>
</svelte:head>

<div class="desk page">
	<a class="back-link" href={myFlightsHref()}>
		<Icon name="back" />
		My flights
	</a>
	<header class="page-head">
		<h1>{app.said?.statistics.title}</h1>
		{#if handle}
			<div class="actions">
				<a class="button-plain" href={pilotHref(handle)}>Your public page</a>
			</div>
		{/if}
	</header>

	{#if phase.is === 'loaded'}
		<PilotTotals statistics={phase.statistics} flightHref={myFlightHref} whole />
		<!-- What the flights say about how the pilot flies, on a page of its own. -->
		<a class="group-card group-row how" href={howYouFlyHref()}>
			<span>{app.said?.statistics.howYouFly}</span>
			<span class="chevron"><Icon name="forward" /></span>
		</a>
	{:else if phase.is === 'signedOut'}
		<div class="unavailable">
			<span class="symbol"><Icon name="person" /></span>
			<h2>Sign in to see your statistics</h2>
			<a class="button-prominent" href={accountHref()}>{app.said?.common.signIn}</a>
		</div>
	{:else if phase.is === 'failed'}
		<div class="unavailable">
			<span class="symbol"><Icon name="warning" /></span>
			<h2>Your statistics could not be fetched</h2>
			<p>{phase.message}</p>
			<button class="button-plain" type="button" onclick={() => (again += 1)}>
				{app.said?.common.tryAgain}
			</button>
		</div>
	{:else}
		<p class="said" role="status">Adding up your flights…</p>
	{/if}
</div>

<style>
	.page {
		min-height: calc(100dvh - var(--nav-top));
	}

	.said {
		margin-bottom: var(--space-3);
		font-size: var(--subheadline);
		color: var(--text-secondary);
	}

	.how {
		margin-top: var(--space-5);
		color: var(--text);
	}

	.chevron {
		color: var(--icon-quiet);
		font-size: 0.75rem;
	}

	@media (hover: hover) {
		.how:hover {
			background: var(--fill);
		}
	}

	.unavailable {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: var(--space-2);
		max-width: 24rem;
		margin: var(--space-6) auto 0;
		text-align: center;
	}

	.symbol {
		color: var(--text-secondary);
		font-size: 2.25rem;
	}

	.unavailable h2 {
		font-size: var(--title);
		font-weight: 600;
	}

	.unavailable p {
		color: var(--text-secondary);
	}
</style>
