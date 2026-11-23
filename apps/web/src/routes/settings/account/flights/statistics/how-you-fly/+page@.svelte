<!--
	How you fly: what the pilot's flights on the account, the private ones
	too, say about how they fly, reached from the foot of the statistics.
	The server keeps each flight's figures of thermals and glides, and the
	core says the observations with its rules (`ownHabitsRequest`,
	`ownTendencies`). Each observation is a card with its title, its words,
	a thin bar where the core gives a share, and a link to the flight that
	shows it most. The words are the core's; nothing here is coloured as
	good or bad, since these are observations and not a verdict.

	On a phone the cards follow each other; on a wide screen they stand in
	columns, in the density of the statistics. Drawn whole-width, as the
	statistics are, out of the settings' frame.
-->
<script lang="ts">
	import { untrack } from 'svelte';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { accountHref, myFlightHref, myStatisticsHref } from '#lib/app/routes.ts';
	import type { Tendencies } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import { reasonFor } from '#lib/logbook/model.svelte.ts';
	import { barPercent, COUNTING, NOTHING_YET, SHOWS_IT_MOST } from '#lib/logbook/tendencies.ts';

	const app = getApp();
	const text = format();

	type Phase =
		| { is: 'loading' }
		| { is: 'loaded'; tendencies: Tendencies }
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
		let current = true;
		if (untrack(() => phase.is) !== 'loaded') phase = { is: 'loading' };
		flights.tendencies().then(
			(tendencies) => {
				if (current) phase = { is: 'loaded', tendencies };
			},
			(error) => {
				if (current) phase = { is: 'failed', message: reasonFor(error) };
			}
		);
		return () => {
			current = false;
		};
	});
</script>

<svelte:head>
	<title>How you fly · Venti</title>
</svelte:head>

<div class="desk page">
	<a class="back-link" href={myStatisticsHref()}>
		<Icon name="back" />
		Statistics
	</a>
	<header class="page-head">
		<h1>How you fly</h1>
	</header>

	{#if phase.is === 'loaded'}
		{@const tendencies = phase.tendencies}
		{#if tendencies.observations.length > 0}
			<ul class="cards">
				{#each tendencies.observations as observation (observation.kind)}
					<li>
						<div class="card" role="group" aria-labelledby="tendency-{observation.kind}">
							<h2 id="tendency-{observation.kind}">{observation.title}</h2>
							<p>{observation.words}</p>
							{#if observation.share !== undefined}
								<!-- The words say what the share is of; the bar only shows it. -->
								<span class="track" aria-hidden="true">
									<span class="bar" style:width="{barPercent(observation.share)}%"></span>
								</span>
							{/if}
							{#if observation.flightId}
								<a class="flight" href={myFlightHref(observation.flightId)}>
									<span class="pair">
										<span>{SHOWS_IT_MOST}</span>
										{#if observation.flightUnixS !== undefined}
											<span class="lesser">{text.date(observation.flightUnixS)}</span>
										{/if}
									</span>
									<span class="chevron"><Icon name="forward" /></span>
								</a>
							{/if}
						</div>
					</li>
				{/each}
			</ul>
		{:else}
			<p class="nothing">{NOTHING_YET}</p>
		{/if}
		<p class="footnote">{tendencies.note}</p>
	{:else if phase.is === 'signedOut'}
		<div class="unavailable">
			<span class="symbol"><Icon name="person" /></span>
			<h2>Sign in to see how you fly</h2>
			<a class="button-prominent" href={accountHref()}>Sign in</a>
		</div>
	{:else if phase.is === 'failed'}
		<div class="unavailable">
			<span class="symbol"><Icon name="warning" /></span>
			<h2>Your flights could not be read</h2>
			<p>{phase.message}</p>
			<button class="button-plain" type="button" onclick={() => (again += 1)}>
				{app.said?.common.tryAgain ?? 'Try again'}
			</button>
		</div>
	{:else}
		<p class="nothing" role="status">{COUNTING}</p>
	{/if}
</div>

<style>
	.page {
		min-height: calc(100dvh - var(--nav-top));
	}

	.cards {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		list-style: none;
	}

	.card {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-4);
		background: var(--surface);
		border-radius: var(--radius-card);
	}

	h2 {
		font-size: var(--subheadline);
		font-weight: 600;
		color: var(--text-secondary);
	}

	/* A thin bar in the primary colour: a share, never good or bad. */
	.track {
		display: block;
		height: 0.25rem;
		border-radius: var(--radius-pill);
		background: var(--fill);
		overflow: hidden;
	}

	.bar {
		display: block;
		height: 100%;
		background: var(--accent);
	}

	.flight {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		margin: 0 calc(var(--space-2) * -1) calc(var(--space-2) * -1);
		padding: var(--space-2);
		border-radius: var(--radius-s);
		color: var(--link);
	}

	.pair {
		flex: 1;
		display: flex;
		flex-direction: column;
	}

	.lesser {
		font-size: var(--subheadline);
		color: var(--text-secondary);
	}

	.chevron {
		color: var(--icon-quiet);
		font-size: 0.75rem;
	}

	@media (hover: hover) {
		.flight:hover {
			background: var(--fill);
		}
	}

	.footnote {
		margin-top: var(--space-4);
		font-size: var(--caption);
		color: var(--text-secondary);
	}

	.nothing {
		margin-top: var(--space-6);
		color: var(--text-secondary);
		text-align: center;
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
		color: var(--text);
	}

	.unavailable p {
		color: var(--text-secondary);
	}

	/* Wide: the cards in columns, as dense as the statistics. */
	@media (min-width: 56rem) {
		.cards {
			display: grid;
			grid-template-columns: repeat(auto-fill, minmax(20rem, 1fr));
			align-items: start;
		}

		.card {
			padding: var(--space-3);
			border: 1px solid var(--color-border);
			border-radius: var(--radius-m);
		}
	}
</style>
