<!--
	What a visitor sees first of what is flown on Venti: the best flights of
	the day, or the newest where nobody shared one today, and the first three
	of this year's ranking, each with the way to the rest. It stands at the
	head of everyone's flights and under an empty logbook.

	What another pilot wrote, a name or a glider, is set as text.
-->
<script lang="ts">
	import { getApp } from '#lib/app/app.svelte.ts';
	import { everyoneHref, pilotHref, rankingHref } from '#lib/app/routes.ts';
	import { format } from '#lib/design/format.ts';
	import type { FrontState } from './model.svelte';
	import { queryOf } from './pages';
	import PublicFlightRow from './PublicFlightRow.svelte';

	interface Props {
		front: FrontState;
		/**
		 * Whether the way to everyone's flights is shown, and the newest
		 * flights when nobody flew today: not where the page is that list.
		 */
		toEveryone?: boolean;
		/** How many of the ranking's pilots are shown: three, or ten in a wide screen's side column. */
		ranked?: number;
	}

	let { front, toEveryone = true, ranked = 3 }: Props = $props();
	const top = $derived(front.top.slice(0, ranked));

	const app = getApp();
	const text = format();
	const todaysBest = $derived(
		everyoneHref(queryOf({ order: 'best', from: front.today, to: front.today }))
	);
</script>

{#if front.known && !front.absent && !front.problem}
	<div class="front">
		{#if front.ofToday || toEveryone}
			<section aria-labelledby="front-flights">
				<div class="head">
					<h2 id="front-flights">{front.ofToday ? 'Best of today' : 'Flown lately'}</h2>
					{#if front.ofToday}
						<a class="more" href={todaysBest}>All of today</a>
					{:else}
						<a class="more" href={everyoneHref()}>{app.said?.pages.everyonesFlights}</a>
					{/if}
				</div>
				{#if front.flights.length > 0}
					{#if !front.ofToday}
						<p class="note">Nobody has shared a flight of today yet. The newest:</p>
					{/if}
					<ul class="card" aria-label={front.ofToday ? 'Best of today' : 'Flown lately'}>
						{#each front.flights as flight (flight.id)}
							<PublicFlightRow {flight} named />
						{/each}
					</ul>
				{:else}
					<p class="note">Nobody has shared a flight on Venti yet.</p>
				{/if}
			</section>
		{/if}

		{#if top.length > 0}
			<section aria-labelledby="front-top">
				<div class="head">
					<h2 id="front-top">Top of {front.season}</h2>
					<a class="more" href={rankingHref()}>The whole ranking</a>
				</div>
				<ol class="card top" aria-label="Top of {front.season}">
					{#each top as pilot (pilot.handle)}
						<li>
							<span class="place numeric">
								<span class="visually-hidden">Place</span>
								{pilot.place}
							</span>
							<a class="name" href={pilotHref(pilot.handle)}>{pilot.name}</a>
							<span class="points numeric">
								{text.points(pilot.points)}
								<span class="unit">points</span>
							</span>
						</li>
					{/each}
				</ol>
			</section>
		{/if}
	</div>
{/if}

<style>
	.front {
		display: flex;
		flex-direction: column;
		gap: 1.5rem;
		margin-bottom: 2rem;
	}

	.head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 1rem;
		margin-bottom: 0.5rem;
	}

	h2 {
		font-size: var(--title);
	}

	.more {
		color: var(--link);
		font-size: var(--subheadline);
		font-weight: 500;
	}

	.note {
		margin-bottom: 0.5rem;
		color: var(--text-secondary);
		font-size: var(--subheadline);
	}

	.card {
		/* The width of a number's column in a wide row, as in `PublicFlightList`. */
		--column: 6.5rem;
		background: var(--surface);
		border-radius: var(--radius-card);
		overflow: hidden;
	}

	.top li {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		min-height: var(--target);
		padding: 0.5rem 1rem;
	}

	.top li + li {
		box-shadow: inset 0 1px 0 var(--separator);
	}

	.place {
		width: 1.5rem;
		font-weight: 600;
		text-align: center;
		color: var(--accent);
	}

	.name {
		flex: 1;
		min-width: 0;
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.points {
		font-weight: 600;
	}

	.unit {
		font-size: var(--footnote);
		font-weight: 400;
		color: var(--text-secondary);
	}

	@media (hover: hover) {
		.more:hover,
		.name:hover {
			text-decoration: underline;
		}
	}
</style>
