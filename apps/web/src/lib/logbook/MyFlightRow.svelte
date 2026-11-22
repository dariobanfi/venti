<!--
	One of the pilot's flights in their list, made to be told apart at a
	glance: the take-off's time first and large, the track's shape on a
	little map of its own, where it took off and on what, and the airtime
	large at the right with the distance under it. The whole row opens the
	flight.

	The take-off's name is the server's: the site nearest the take-off,
	within 500 m (`takeoff_site`), as a flight's page names it; without one
	the place is written as its position. The glider is the file's own
	(`HFGTY`, as the server reads it), set as text.
-->
<script lang="ts">
	import { myFlightHref } from '#lib/app/routes.ts';
	import { distance, shape } from '#lib/community/words.ts';
	import type { OwnFlight } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import TrackThumbnail from './TrackThumbnail.svelte';

	interface Props {
		flight: OwnFlight;
	}

	let { flight }: Props = $props();

	const text = format();
	const row = $derived(flight.row);
	const triangle = $derived(shape(row));
	const where = $derived(
		row.takeoffName ?? (row.takeoff ? text.place(row.takeoff.lat, row.takeoff.lon) : undefined)
	);
	/** Who sees it, where anyone but the pilot does. */
	const seen = $derived(
		flight.sharing.visibility === 'public'
			? 'Public'
			: flight.sharing.visibility === 'unlisted'
				? 'With a link'
				: undefined
	);
</script>

<li>
	<span class="time numeric">
		<span class="visually-hidden">Took off at</span>
		{text.time(row.startUnixS)}
	</span>
	<span class="thumb"><TrackThumbnail outline={row.outline} large /></span>
	<!-- The flight's link is stretched over the whole row. -->
	<a class="flight where" href={myFlightHref(row.id)}>
		<span class="sign"><Icon name="takeoff" /></span>
		<span class="name">{where ?? text.day(row.startUnixS)}</span>
	</a>
	<span class="glider">
		{#if row.glider}
			<span class="sign"><Icon name="glider" /></span>
			<span class="name">{row.glider}</span>
		{/if}
		{#if seen}
			<span class="seen">{seen}</span>
		{/if}
	</span>
	<span class="airtime numeric">
		<span class="visually-hidden">Airtime</span>
		{text.hoursMinutes(row.airtimeS)}<span class="unit">h</span>
	</span>
	<span class="distance numeric">
		<span class="visually-hidden">Distance</span>
		{#if triangle}
			<span class="shape" role="img" aria-label={triangle}><Icon name="triangle" /></span>
		{/if}
		{distance(row, text)} km
	</span>
	<span class="extra points numeric">
		<span class="visually-hidden">Points</span>
		{row.score ? `${text.points(row.score.points)} pts` : '–'}
	</span>
	<span class="extra highest numeric">
		<span class="visually-hidden">Highest</span>
		{text.metres(row.maxAltitudeM)} m
	</span>
	<span class="chevron"><Icon name="forward" /></span>
</li>

<style>
	/* Narrow: the track's little map at the left; beside it the take-off's
	   time and the airtime, both large, then where and on what, with the
	   distance. */
	li {
		position: relative;
		display: grid;
		grid-template-columns: auto minmax(0, 1fr) auto auto;
		grid-template-areas:
			'thumb time airtime chevron'
			'thumb where where chevron'
			'thumb glider distance chevron';
		align-items: center;
		column-gap: var(--space-3);
		row-gap: 0.125rem;
		padding: var(--space-3) var(--space-3) var(--space-3) var(--space-4);
	}

	/* A hairline between rows, starting where the words do. */
	li:not(:first-child)::before {
		content: '';
		position: absolute;
		top: 0;
		left: var(--space-4);
		right: 0;
		border-top: 0.5px solid var(--separator);
	}

	.thumb {
		grid-area: thumb;
	}

	/* The take-off's time leads, large: the pilot's day is read by it. */
	.time {
		grid-area: time;
		font-size: var(--number);
		font-weight: 600;
		line-height: 1.1;
	}

	.where {
		grid-area: where;
	}

	.glider {
		grid-area: glider;
		font-size: var(--subheadline);
		color: var(--text-secondary);
	}

	.where,
	.glider {
		display: flex;
		align-items: center;
		gap: 0.25rem;
		min-width: 0;
	}

	.name {
		min-width: 0;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.flight {
		font-weight: 600;
		color: var(--text);
	}

	.sign {
		flex: none;
		display: inline-flex;
		color: var(--icon-quiet);
		font-size: 0.875em;
	}

	.seen {
		flex: none;
		margin-left: var(--space-2);
		font-size: var(--footnote);
	}

	/* The whole row opens the flight. */
	.flight::after {
		content: '';
		position: absolute;
		inset: 0;
	}

	/* The airtime large, the distance under it. */
	.airtime {
		grid-area: airtime;
		justify-self: end;
		font-size: var(--number);
		font-weight: 600;
		line-height: 1.1;
		white-space: nowrap;
	}

	.unit {
		margin-left: 0.125rem;
		font-size: var(--subheadline);
		font-weight: 500;
		color: var(--text-secondary);
	}

	.distance {
		grid-area: distance;
		justify-self: end;
		font-size: var(--subheadline);
		color: var(--text-secondary);
		white-space: nowrap;
	}

	.shape {
		display: inline-block;
		font-size: 0.5rem;
		vertical-align: 0.05em;
	}

	.extra {
		display: none;
	}

	.chevron {
		grid-area: chevron;
		color: var(--icon-quiet);
		font-size: 0.75rem;
	}

	@media (hover: hover) {
		li:hover {
			background: var(--fill);
		}
	}

	li:has(.flight:active) {
		background: var(--fill-strong);
	}

	.flight:focus-visible {
		outline: none;
	}

	.flight:focus-visible::after {
		outline: 0.1875rem solid var(--color-focus);
		outline-offset: -0.1875rem;
		border-radius: var(--radius-card);
	}

	/* Wide: denser, one line of columns: the time, the shape, where and on
	   what, then the airtime, the distance, the points and the highest. */
	@media (min-width: 56rem) {
		li {
			grid-template-columns: 4.5rem auto minmax(0, 1fr) 5rem 5.5rem 5.5rem 4.5rem auto;
			grid-template-areas:
				'time thumb where airtime distance points highest chevron'
				'time thumb glider airtime distance points highest chevron';
			padding-block: var(--space-2);
		}

		.time,
		.airtime {
			font-size: var(--title);
		}

		.distance,
		.extra {
			display: block;
			justify-self: end;
			font-size: var(--body);
			color: var(--text);
			white-space: nowrap;
		}

		.points {
			grid-area: points;
		}

		.highest {
			grid-area: highest;
		}
	}
</style>
