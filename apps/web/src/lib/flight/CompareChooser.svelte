<!--
	"Lay a flight beside it": the flight to compare the one on screen with.
	What other pilots show to everyone and flew around the same place within
	twelve hours, and the pilot's own flights, those nearest in time first.
	The words are the iPhone's `CompareChooser`.

	What another pilot wrote, a name or a glider, is set as text.
-->
<script lang="ts">
	import { getApp } from '#lib/app/app.svelte.ts';
	import { words } from '#lib/community/pages.ts';
	import { distance } from '#lib/community/words.ts';
	import type { FlightRef, FlightView, PublicFlight } from '#lib/core/index.ts';
	import { flightFormat, format } from '#lib/design/format.ts';
	import Sheet from '#lib/design/Sheet.svelte';

	interface Props {
		open: boolean;
		/** The flight on screen. */
		flight: FlightView;
		/** Its address on Venti, to leave it out of the lists. */
		publicId?: string;
		onchoose: (ref: FlightRef) => void;
		onclose: () => void;
	}

	let { open, flight, publicId, onchoose, onclose }: Props = $props();

	const app = getApp();
	const text = format();

	/** What others flew there that day, once asked; or why not. */
	let others = $state.raw<{ flights?: PublicFlight[]; problem?: string }>();

	$effect(() => {
		if (!open || !app.account?.hasServer || !app.everyone) return;
		let current = true;
		others = undefined;
		app.everyone.flownAround(flight.bounds, flight.startUnixS).then(
			(found) => {
				if (!current) return;
				// The flight on screen may be one of them.
				others = {
					flights: (found ?? []).filter(
						(other) => other.id !== publicId && other.startUnixS !== flight.startUnixS
					)
				};
			},
			(error) => {
				if (current) others = { problem: words(error) };
			}
		);
		return () => {
			current = false;
		};
	});

	/** The pilot's other flights on Venti, those nearest in time first: no more than fifty. */
	const own = $derived(
		(app.myFlights?.flights.map((flight) => flight.row) ?? [])
			.filter((entry) => entry.id !== publicId && entry.startUnixS !== flight.startUnixS)
			.toSorted(
				(one, two) =>
					Math.abs(one.startUnixS - flight.startUnixS) -
					Math.abs(two.startUnixS - flight.startUnixS)
			)
			.slice(0, 50)
	);
</script>

<Sheet {open} title="Lay a flight beside it" {onclose}>
	<div class="chooser">
		{#if app.account?.hasServer}
			<section class="group" aria-labelledby="compare-others">
				<h2 class="group-title" id="compare-others">Flown here that day</h2>
				{#if others?.flights && others.flights.length > 0}
					<ul class="group-card" aria-labelledby="compare-others">
						{#each others.flights as other (other.id)}
							{@const when = flightFormat(other)}
							<li>
								<button
									class="group-row"
									type="button"
									onclick={() => onchoose({ public: other.id })}
								>
									<span class="who">
										<span class="name">{other.pilot}</span>
										<span class="lesser">
											{when.day(other.startUnixS)} · {when.time(other.startUnixS)}
										</span>
									</span>
									<span class="lesser numeric">
										{text.kilometres(other.score?.distanceM ?? other.trackLengthM)} km · {text.hoursMinutes(
											other.airtimeS
										)} h
									</span>
								</button>
							</li>
						{/each}
					</ul>
				{:else if others?.flights}
					<p class="group-note first">Nobody shows a flight from around here on that day.</p>
				{:else if others?.problem}
					<p class="group-note first">{others.problem}</p>
				{:else}
					<p class="group-note first" role="status">Fetching the flights…</p>
				{/if}
				<p class="group-note">
					Flights that other pilots show to everyone, begun within twelve hours of this one around
					where it was flown.
				</p>
			</section>
		{/if}

		<section class="group" aria-labelledby="compare-own">
			<h2 class="group-title" id="compare-own">Your flights</h2>
			{#if own.length > 0}
				<ul class="group-card" aria-labelledby="compare-own">
					{#each own as entry (entry.id)}
						{@const when = flightFormat(entry)}
						<li>
							<button class="group-row" type="button" onclick={() => onchoose({ own: entry.id })}>
								<span>
									{when.day(entry.startUnixS)}
									<span class="lesser">{when.time(entry.startUnixS)}</span>
								</span>
								<span class="lesser numeric">
									{distance(entry, text)} km · {text.hoursMinutes(entry.airtimeS)} h
								</span>
							</button>
						</li>
					{/each}
				</ul>
			{:else}
				<p class="group-note first">
					{app.account?.signedIn
						? 'You have no other flight on Venti.'
						: 'Sign in to lay your own flights beside it.'}
				</p>
			{/if}
		</section>
	</div>
</Sheet>

<style>
	.chooser {
		padding: 0 var(--gutter) 1.5rem;
	}

	.chooser > .group:first-child {
		margin-top: 0.5rem;
	}

	.first {
		padding-top: 0;
	}

	.lesser {
		color: var(--text-secondary);
	}

	.who {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}

	.name {
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.group-row {
		color: var(--text);
	}
</style>
