<!--
	The pilots of a season as a table, on a wide screen: the place, the
	name, the points, what counts and the flights that count, each a link to
	the flight. The headings of place, pilot and points sort.

	What another pilot wrote, a name, is set as text.
-->
<script lang="ts">
	import { pilotHref, publicFlightHref } from '#lib/app/routes.ts';
	import type { RankedPilot } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import SortHeading from '#lib/design/SortHeading.svelte';
	import { type Sort, sortBy, sorted } from '#lib/design/sort.ts';
	import { distance, shape } from './words';

	interface Props {
		/** What the table is called, for a screen reader. */
		label: string;
		pilots: RankedPilot[];
	}

	let { label, pilots }: Props = $props();

	const text = format();

	type Key = 'place' | 'pilot' | 'points';
	let sort = $state<Sort<Key>>();
	const VALUE: Record<Key, (pilot: RankedPilot) => number | string> = {
		// The first place first, as a ranking is read.
		place: (pilot) => -pilot.place,
		pilot: (pilot) => pilot.name,
		points: (pilot) => pilot.points
	};
	const rows = $derived(sorted(pilots, sort, (pilot, key) => VALUE[key](pilot)));
	const onsort = (key: Key) => (sort = sortBy(sort, key, ['pilot']));
</script>

<table aria-label={label}>
	<thead>
		<tr>
			<SortHeading key="place" {sort} {onsort} numeric>Place</SortHeading>
			<SortHeading key="pilot" {sort} {onsort}>Pilot</SortHeading>
			<SortHeading key="points" {sort} {onsort} numeric>Points</SortHeading>
			<th scope="col">What counts</th>
			<th scope="col">Flights that count</th>
		</tr>
	</thead>
	<tbody>
		{#each rows as pilot (pilot.handle)}
			<tr>
				<td class="number place" class:podium={pilot.place <= 3}>{pilot.place}</td>
				<td class="name"><a href={pilotHref(pilot.handle)}>{pilot.name}</a></td>
				<td class="number">{text.points(pilot.points)}</td>
				<td class="lesser">{pilot.counted}</td>
				<td>
					<ul class="flights" aria-label="The flights of {pilot.name} that count">
						{#each pilot.flights as flight (flight.id)}
							{@const triangle = shape(flight)}
							<li>
								<a href={publicFlightHref(flight.id)}>
									{text.day(flight.startUnixS)} ·
									<span class="numeric">
										{#if triangle}
											<span class="shape" role="img" aria-label={triangle}>
												<Icon name="triangle" />
											</span>
										{/if}
										{distance(flight, text)} km ·
										<span class="visually-hidden">Points</span>
										{flight.score ? text.points(flight.score.points) : '–'}
									</span>
								</a>
							</li>
						{/each}
					</ul>
				</td>
			</tr>
		{/each}
	</tbody>
</table>

<style>
	table {
		width: 100%;
		border-collapse: collapse;
	}

	td {
		height: 2rem;
		padding: var(--space-1) var(--space-2);
		vertical-align: top;
		line-height: 1.5rem;
	}

	tbody tr + tr td {
		box-shadow: inset 0 1px 0 var(--separator);
	}

	.number {
		text-align: right;
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}

	.place {
		width: 3.5rem;
		font-weight: 600;
		color: var(--text-secondary);
	}

	/* The first three are the ones looked for. */
	.podium {
		color: var(--link);
	}

	.name {
		white-space: nowrap;
		font-weight: 600;
	}

	.name a {
		color: inherit;
	}

	.lesser {
		color: var(--text-secondary);
		white-space: nowrap;
	}

	.flights {
		display: flex;
		flex-wrap: wrap;
		gap: 0 var(--space-4);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.flights a {
		color: var(--link);
		white-space: nowrap;
	}

	.shape {
		display: inline-flex;
		vertical-align: -0.125rem;
	}

	.shape :global(svg) {
		width: 0.875rem;
		height: 0.875rem;
	}
</style>
