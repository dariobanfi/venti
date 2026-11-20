<!--
	One public flight in a list: its shape, who flew it and when, and how far
	and how long. The whole row opens the flight; the pilot's name is a link
	of its own to that pilot's flights.

	What another pilot wrote, a name or a glider, is set as text. The core has
	cut it to one line; nothing of it is ever read as markup.
-->
<script lang="ts">
	import { pilotHref, publicFlightHref } from '#lib/app/routes.ts';
	import type { PublicFlight } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import TrackThumbnail from '#lib/logbook/TrackThumbnail.svelte';
	import { detail, distance, shape } from './words';

	interface Props {
		flight: PublicFlight;
		/** Whether the row names the pilot: not in a list of one pilot's flights. */
		named: boolean;
	}

	let { flight, named }: Props = $props();

	const text = format();
	const triangle = $derived(shape(flight));
</script>

<li>
	<TrackThumbnail outline={flight.outline} />
	<span class="who">
		<!-- The flight's link is stretched over the whole row. -->
		{#if named}
			<a class="pilot" href={pilotHref(flight.handle)}>{flight.pilot}</a>
			<a class="flight" href={publicFlightHref(flight.id)}>{detail(flight, text)}</a>
		{:else}
			<!-- Among one pilot's flights the day leads, as in the logbook. -->
			<a class="flight leads" href={publicFlightHref(flight.id)}>
				<span class="day">{text.day(flight.startUnixS)}</span>
				{#if flight.glider}
					<span class="glider">{flight.glider}</span>
				{/if}
			</a>
		{/if}
	</span>
	<span class="figures numeric">
		<!-- The scored cross-country distance; a triangle is marked. -->
		<span class="figure distance">
			<span class="visually-hidden">Distance</span>
			{#if triangle}
				<span class="shape" role="img" aria-label={triangle}><Icon name="triangle" /></span>
			{/if}
			{distance(flight, text)} km
		</span>
		<span class="figure airtime">
			<span class="visually-hidden">Airtime</span>
			{text.hoursMinutes(flight.airtimeS)} h
		</span>
		<span class="figure extra">
			<span class="visually-hidden">Points</span>
			{flight.score ? text.points(flight.score.points) : '–'}
		</span>
		<span class="figure extra">
			<span class="visually-hidden">Highest</span>
			{text.metres(flight.maxAltitudeM)} m
		</span>
	</span>
	<span class="chevron"><Icon name="forward" /></span>
</li>

<style>
	li {
		position: relative;
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.625rem 0.75rem 0.625rem 1rem;
	}

	/* A hairline between rows, starting where the text does. */
	li:not(:first-child)::before {
		content: '';
		position: absolute;
		top: 0;
		left: 5rem;
		right: 0;
		border-top: 0.5px solid var(--separator);
	}

	.who {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 0.125rem;
	}

	.pilot,
	.flight,
	.day,
	.glider {
		max-width: 100%;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	/* Above the flight's link, which covers the row: a target of its own. */
	.pilot {
		position: relative;
		z-index: 1;
		font-weight: 600;
	}

	.flight {
		font-size: var(--subheadline);
		color: var(--text-secondary);
	}

	/* Without a name over it, the day is the row's first line and the glider
	   its second. */
	.flight.leads {
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
		overflow: visible;
	}

	.day {
		font-size: var(--body);
		font-weight: 600;
		color: var(--text);
	}

	/* The whole row opens the flight. */
	.flight::after {
		content: '';
		position: absolute;
		inset: 0;
	}

	/* Narrow: distance over airtime, at the row's right edge. */
	.figures {
		display: flex;
		flex-direction: column;
		align-items: flex-end;
		gap: 0.125rem;
		white-space: nowrap;
	}

	.distance {
		font-weight: 600;
	}

	.airtime {
		font-size: var(--subheadline);
		color: var(--text-secondary);
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
		color: var(--icon-quiet);
		font-size: 0.75rem;
	}

	@media (hover: hover) {
		li:hover {
			background: var(--fill);
		}

		.pilot:hover {
			text-decoration: underline;
		}
	}

	li:has(.flight:active) {
		background: var(--fill-strong);
	}

	/* The ring goes round the row the link covers, inside the card's corners. */
	.flight:focus-visible {
		outline: none;
	}

	.flight:focus-visible::after {
		outline: 0.1875rem solid var(--color-focus);
		outline-offset: -0.1875rem;
		border-radius: 0.5rem;
	}

	/* Wide: the numbers in columns under the headings of `PublicFlightList`. */
	@media (min-width: 56rem) {
		li {
			padding-right: 1rem;
		}

		.figures {
			flex-direction: row;
			align-items: baseline;
			gap: 0;
		}

		.figure,
		.figure.extra {
			display: block;
			width: var(--column);
			text-align: right;
			font-size: var(--body);
			font-weight: 400;
			color: var(--text);
		}

		.chevron {
			display: none;
		}
	}
</style>
