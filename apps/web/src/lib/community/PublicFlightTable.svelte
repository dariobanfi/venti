<!--
	Other pilots' flights as a table, on a wide screen: rows 32 px high with
	the pilot, the day, the take-off, the glider, the kind of route and the
	figures in columns that line up, under a heading that sorts what has
	been fetched so far. Wider than its card, it scrolls sideways in the
	card, never the page (`PublicFlightList.svelte`). A click
	anywhere on a row opens the flight; the pilot's name and the take-off
	are links of their own.

	The take-off is the name of the site it took off from, where the server
	has one there, and otherwise the place; either leads to what else was
	flown from there.

	What another pilot wrote, a name or a glider, is set as text.
-->
<script lang="ts" module>
	/** The core's lines of cheers and comments for the flights of a feed, and what the two are called. */
	export interface Said {
		cheersTitle: string;
		commentsTitle: string;
		of: Record<string, { cheers: string; comments: string } | undefined>;
	}
</script>

<script lang="ts">
	import { goto } from '$app/navigation';
	import { pilotHref, placeHref, publicFlightHref } from '#lib/app/routes.ts';
	import type { PublicFlight } from '#lib/core/index.ts';
	import { flightFormat, format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import SortHeading from '#lib/design/SortHeading.svelte';
	import { type Sort, sortBy, sorted } from '#lib/design/sort.ts';
	import { getApp } from '#lib/app/app.svelte.ts';
	import Flag from './Flag.svelte';
	import PilotFace from './PilotFace.svelte';
	import { distance, gliderLine, kindOf, shape } from './words';

	interface Props {
		/** What the table is called, for a screen reader. */
		label: string;
		flights: PublicFlight[];
		/** Whether the table names the pilot: not in a table of one pilot's flights. */
		named: boolean;
		/**
		 * What other pilots said of each flight, by its address: the core's
		 * lines of its cheers and its comments, in a feed, under the core's
		 * two headings.
		 */
		said?: Said;
	}

	let { label, flights, named, said }: Props = $props();

	const text = format();
	/** The glider classes in the core's names. */
	const classes = $derived(getApp().said?.gliderClasses);

	type Key =
		'date' | 'pilot' | 'glider' | 'kind' | 'distance' | 'airtime' | 'points' | 'highest' | 'climb';
	const WORDS: Key[] = ['pilot', 'glider', 'kind'];
	let sort = $state<Sort<Key>>();

	const VALUE: Record<Key, (flight: PublicFlight) => number | string> = {
		date: (flight) => flight.startUnixS,
		pilot: (flight) => flight.pilot,
		glider: (flight) => gliderLine(flight, classes) ?? '',
		kind: (flight) => kindOf(flight),
		distance: (flight) => flight.score?.distanceM ?? flight.trackLengthM,
		airtime: (flight) => flight.airtimeS,
		points: (flight) => flight.score?.points ?? 0,
		highest: (flight) => flight.maxAltitudeM,
		climb: (flight) => flight.maxClimbMs
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
			<th class="plain" scope="col">Take-off</th>
			<SortHeading key="glider" {sort} {onsort}>Glider</SortHeading>
			<SortHeading key="distance" {sort} {onsort} numeric>Distance</SortHeading>
			<SortHeading key="kind" {sort} {onsort}>Kind</SortHeading>
			<SortHeading key="points" {sort} {onsort} numeric>Points</SortHeading>
			<SortHeading key="airtime" {sort} {onsort} numeric>Airtime</SortHeading>
			<SortHeading key="highest" {sort} {onsort} numeric>Highest</SortHeading>
			<SortHeading key="climb" {sort} {onsort} numeric>Best climb</SortHeading>
			{#if said}
				<th class="plain" scope="col">{said.cheersTitle}</th>
				<th class="plain" scope="col">{said.commentsTitle}</th>
			{/if}
		</tr>
	</thead>
	<tbody>
		{#each rows as flight (flight.id)}
			{@const triangle = shape(flight)}
			<tr onclick={(event) => open(event, flight)}>
				{#if named}
					<td class="words">
						<PilotFace picture={flight.pilotPicture} name={flight.pilot} />
						<a class="pilot" href={pilotHref(flight.handle)}>{flight.pilot}</a>
						<Flag code={flight.pilotCountry} />
					</td>
				{/if}
				<td>
					<a class="flight" href={publicFlightHref(flight.id)}
						>{flightFormat(flight).day(flight.startUnixS)}</a
					>
				</td>
				<td class="words lesser">
					<Flag code={flight.takeoffCountry} />
					{#if flight.takeoff}
						<a class="place" href={placeHref(flight.takeoff, flight.takeoffSiteId ?? '')}>
							{flight.takeoffName ?? text.place(flight.takeoff.lat, flight.takeoff.lon)}
						</a>
					{:else if flight.takeoffName}
						{flight.takeoffName}
					{/if}
				</td>
				<td class="words">{gliderLine(flight, classes) ?? ''}</td>
				<td class="number">
					{#if triangle}
						<span class="shape" aria-hidden="true"><Icon name="triangle" /></span>
					{/if}
					{distance(flight, text)} km
				</td>
				<td class="lesser">{kindOf(flight)}</td>
				<td class="number strong">{flight.score ? text.points(flight.score.points) : '–'}</td>
				<td class="number">{text.hoursMinutes(flight.airtimeS)} h</td>
				<td class="number">{text.metres(flight.maxAltitudeM)} m</td>
				<td class="number">{text.climb(flight.maxClimbMs)} m/s</td>
				{#if said}
					<td class="lesser">{said.of[flight.id]?.cheers ?? ''}</td>
					<td class="lesser">{said.of[flight.id]?.comments ?? ''}</td>
				{/if}
			</tr>
		{/each}
	</tbody>
</table>

<style>
	table {
		width: 100%;
		border-collapse: collapse;
	}

	thead :global(th) {
		background: var(--surface);
	}

	/* A heading that does not sort, as those that do look. */
	.plain {
		height: 2rem;
		padding: 0 var(--space-2);
		text-align: left;
		white-space: nowrap;
		font-size: var(--subheadline);
		font-weight: 600;
		color: var(--text-secondary);
		box-shadow: inset 0 -1px 0 var(--color-border);
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

	/* The pilot's face before the name, as small as the row. */
	.words :global(.face) {
		width: 1.5rem;
		height: 1.5rem;
		margin-right: var(--space-2);
		vertical-align: middle;
	}

	.number {
		text-align: right;
	}

	.pilot,
	.strong {
		font-weight: 600;
	}

	.lesser {
		color: var(--text-secondary);
	}

	.place {
		color: var(--link);
		font-variant-numeric: tabular-nums;
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
