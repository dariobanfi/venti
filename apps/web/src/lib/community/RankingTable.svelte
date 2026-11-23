<!--
	The pilots of a season as a table, on a wide screen: the place, the
	name, the points, what counts, and each flight that counts in a small
	column of its own, the best first, as a league's ranking has them: its
	points, a link to the flight, over its day and distance. The headings of
	place, pilot and points sort.

	What another pilot wrote, a name, is set as text.
-->
<script lang="ts">
	import { pilotHref, publicFlightHref } from '#lib/app/routes.ts';
	import type { RankedPilot } from '#lib/core/index.ts';
	import { flightFormat, format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import PilotFace from './PilotFace.svelte';
	import SortHeading from '#lib/design/SortHeading.svelte';
	import { type Sort, sortBy, sorted } from '#lib/design/sort.ts';
	import { distance, ordinal, shape } from './words';

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
	/** As many columns of counted flights as the pilot with the most has. */
	const counts = $derived(
		Array.from({ length: Math.max(0, ...pilots.map((pilot) => pilot.flights.length)) }, (_, i) => i)
	);
</script>

<table aria-label={label}>
	<caption class="visually-hidden">
		Each flight that counts has a column of its own, the best first.
	</caption>
	<thead>
		<tr>
			<SortHeading key="place" {sort} {onsort} numeric>Place</SortHeading>
			<SortHeading key="pilot" {sort} {onsort}>Pilot</SortHeading>
			<SortHeading key="points" {sort} {onsort} numeric>Points</SortHeading>
			<th scope="col">What counts</th>
			{#each counts as i (i)}
				<th class="number" scope="col">{ordinal(i + 1)}</th>
			{/each}
		</tr>
	</thead>
	<tbody>
		{#each rows as pilot (pilot.handle)}
			<tr>
				<td class="number place" class:podium={pilot.place <= 3}>{pilot.place}</td>
				<td class="name">
					<PilotFace picture={pilot.picturePath} name={pilot.name} />
					<a href={pilotHref(pilot.handle)}>{pilot.name}</a>
				</td>
				<td class="number">{text.points(pilot.points)}</td>
				<td class="lesser">{pilot.counted}</td>
				{#each counts as i (i)}
					{@const flight = pilot.flights[i]}
					{#if flight}
						{@const triangle = shape(flight)}
						<td class="counted number">
							<a href={publicFlightHref(flight.id)}>
								<span class="visually-hidden">Points</span>
								{flight.score ? text.points(flight.score.points) : '–'}
							</a>
							<span class="lesser small">
								{flightFormat(flight).date(flight.startUnixS)} ·
								{#if triangle}
									<span class="shape" role="img" aria-label={triangle}>
										<Icon name="triangle" />
									</span>
								{/if}
								{distance(flight, text)} km
							</span>
						</td>
					{:else}
						<td></td>
					{/if}
				{/each}
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

	/* The pilot's face before the name, as small as the row. */
	.name :global(.face) {
		width: 1.5rem;
		height: 1.5rem;
		margin-right: var(--space-2);
		vertical-align: middle;
	}

	.lesser {
		color: var(--text-secondary);
		white-space: nowrap;
	}

	/* A counted flight: its points, and under them its day and distance. */
	.counted {
		width: 9.5rem;
		line-height: 1.25rem;
		padding-top: var(--space-1);
		padding-bottom: var(--space-1);
	}

	.counted a {
		display: block;
		color: var(--link);
		font-weight: 600;
	}

	.small {
		display: block;
		font-size: var(--caption);
		line-height: 1rem;
	}

	/* The headings that do not sort, as those that do look. */
	thead th {
		height: 2rem;
		padding: 0 var(--space-2);
		text-align: left;
		white-space: nowrap;
		font-size: var(--subheadline);
		font-weight: 600;
		color: var(--text-secondary);
		box-shadow: inset 0 -1px 0 var(--color-border);
	}

	thead th.number {
		text-align: right;
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
