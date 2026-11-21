<!--
	Other pilots' flights as a table, on a wide screen: rows 32 px high with
	the day, the pilot, the glider and the figures in columns that line up,
	whose headings sort what has been fetched so far. A click anywhere on a
	row opens the flight; the pilot's name is a link of its own.

	What another pilot wrote, a name or a glider, is set as text.
-->
<script lang="ts">
	import { goto } from '$app/navigation';
	import { pilotHref, publicFlightHref } from '#lib/app/routes.ts';
	import type { PublicFlight } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import SortHeading from '#lib/design/SortHeading.svelte';
	import { type Sort, sortBy, sorted } from '#lib/design/sort.ts';
	import { distance, shape } from './words';

	interface Props {
		/** What the table is called, for a screen reader. */
		label: string;
		flights: PublicFlight[];
		/** Whether the table names the pilot: not in a table of one pilot's flights. */
		named: boolean;
	}

	let { label, flights, named }: Props = $props();

	const text = format();

	type Key = 'date' | 'pilot' | 'glider' | 'distance' | 'airtime' | 'points' | 'highest';
	const WORDS: Key[] = ['pilot', 'glider'];
	let sort = $state<Sort<Key>>();

	const VALUE: Record<Key, (flight: PublicFlight) => number | string> = {
		date: (flight) => flight.startUnixS,
		pilot: (flight) => flight.pilot,
		glider: (flight) => flight.glider ?? '',
		distance: (flight) => flight.score?.distanceM ?? flight.trackLengthM,
		airtime: (flight) => flight.airtimeS,
		points: (flight) => flight.score?.points ?? 0,
		highest: (flight) => flight.maxAltitudeM
	};
	const rows = $derived(sorted(flights, sort, (flight, key) => VALUE[key](flight)));
	const onsort = (key: Key) => (sort = sortBy(sort, key, WORDS));

	/** A click on a row, outside its links, opens the flight. */
	function open(event: MouseEvent, flight: PublicFlight) {
		if ((event.target as Element).closest('a')) return;
		void goto(publicFlightHref(flight.id));
	}
</script>

<table aria-label={label} class="numeric">
	<thead>
		<tr>
			{#if named}
				<SortHeading key="pilot" {sort} {onsort}>Pilot</SortHeading>
			{/if}
			<SortHeading key="date" {sort} {onsort}>Date</SortHeading>
			<SortHeading key="glider" {sort} {onsort}>Glider</SortHeading>
			<SortHeading key="distance" {sort} {onsort} numeric>Distance</SortHeading>
			<SortHeading key="airtime" {sort} {onsort} numeric>Airtime</SortHeading>
			<SortHeading key="points" {sort} {onsort} numeric>Points</SortHeading>
			<SortHeading key="highest" {sort} {onsort} numeric>Highest</SortHeading>
		</tr>
	</thead>
	<tbody>
		{#each rows as flight (flight.id)}
			{@const triangle = shape(flight)}
			<tr onclick={(event) => open(event, flight)}>
				{#if named}
					<td class="words"><a class="pilot" href={pilotHref(flight.handle)}>{flight.pilot}</a></td>
				{/if}
				<td>
					<a class="flight" href={publicFlightHref(flight.id)}>{text.day(flight.startUnixS)}</a>
				</td>
				<td class="words">{flight.glider ?? ''}</td>
				<td class="number">
					{#if triangle}
						<span class="shape" role="img" aria-label={triangle}><Icon name="triangle" /></span>
					{/if}
					{distance(flight, text)} km
				</td>
				<td class="number">{text.hoursMinutes(flight.airtimeS)} h</td>
				<td class="number">{flight.score ? text.points(flight.score.points) : '–'}</td>
				<td class="number">{text.metres(flight.maxAltitudeM)} m</td>
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
		padding: 0 var(--space-2);
		white-space: nowrap;
	}

	tbody tr {
		cursor: pointer;
	}

	tbody tr + tr td {
		box-shadow: inset 0 1px 0 var(--separator);
	}

	.words {
		max-width: 14rem;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.number {
		text-align: right;
	}

	.pilot {
		font-weight: 600;
	}

	a {
		color: inherit;
	}

	.flight {
		color: var(--link);
	}

	.shape {
		display: inline-flex;
		vertical-align: -0.125rem;
		color: var(--text-secondary);
	}

	.shape :global(svg) {
		width: 0.875rem;
		height: 0.875rem;
	}

	@media (hover: hover) {
		tbody tr:hover {
			background: var(--fill);
		}
	}
</style>
