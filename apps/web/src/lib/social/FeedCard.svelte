<!--
	One flight of the feed as a card of Community, made to be told apart at
	a glance as a row of My flights is: the track's shape on a little map of
	its own, the pilot, where they took off and on which day, the airtime
	and the distance large, and the core's lines of cheers and comments.
	The whole card opens the flight; the pilot's name opens their page.
-->
<script lang="ts">
	import { getApp } from '#lib/app/app.svelte.ts';
	import { pilotHref, publicFlightHref } from '#lib/app/routes.ts';
	import Flag from '#lib/community/Flag.svelte';
	import PilotFace from '#lib/community/PilotFace.svelte';
	import { distance, shape } from '#lib/community/words.ts';
	import type { FeedRow } from '#lib/core/index.ts';
	import { flightFormat } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import TrackThumbnail from '#lib/logbook/TrackThumbnail.svelte';

	let { row }: { row: FeedRow } = $props();

	const app = getApp();

	// The clock and the date of the place it was flown.
	const text = $derived(flightFormat(row.flight));
	const flight = $derived(row.flight);
	const triangle = $derived(shape(flight, app.said?.routeKinds));
	const where = $derived(
		flight.takeoffName ??
			(flight.takeoff ? text.place(flight.takeoff.lat, flight.takeoff.lon) : undefined)
	);
	const said = $derived([row.cheersLine, row.commentsLine].filter(Boolean).join(' · '));
</script>

<li class="flight">
	<span class="thumb"><TrackThumbnail outline={flight.outline} large /></span>
	<span class="pilot-line">
		<PilotFace picture={flight.pilotPicture} name={flight.pilot} />
		<a class="pilot" href={pilotHref(flight.handle)}>{flight.pilot}</a>
		<Flag code={flight.pilotCountry} />
	</span>
	<!-- The flight's link is stretched over the whole card. -->
	<a class="open where" href={publicFlightHref(flight.id)}>
		<span class="sign"><Icon name="takeoff" /></span>
		<span class="name">{where ?? text.date(flight.startUnixS)}</span>
		<Flag code={flight.takeoffCountry} />
	</a>
	<span class="day">{text.date(flight.startUnixS)}</span>
	<span class="numbers">
		<span class="number numeric">
			<span class="visually-hidden">Airtime</span>
			{text.hoursMinutes(flight.airtimeS)}<span class="unit">h</span>
		</span>
		<span class="number numeric">
			<span class="visually-hidden">Distance</span>
			{#if triangle}
				<span class="shape" role="img" aria-label={triangle}><Icon name="triangle" /></span>
			{/if}
			{distance(flight, text)}<span class="unit">km</span>
		</span>
	</span>
	{#if said}
		<span class="said">{said}</span>
	{/if}
</li>

<style>
	.flight {
		position: relative;
		display: grid;
		grid-template-columns: auto minmax(0, 1fr);
		grid-template-areas:
			'thumb pilot'
			'thumb where'
			'thumb day'
			'numbers numbers'
			'said said';
		align-items: center;
		/* A card beside a taller one keeps its lines together at the top. */
		align-content: start;
		column-gap: var(--space-3);
		row-gap: 0.125rem;
		min-width: 0;
		padding: var(--space-3) var(--space-4) var(--space-4);
		background: var(--surface);
		border: 1px solid var(--separator);
		border-radius: var(--radius-card);
	}

	.thumb {
		grid-area: thumb;
	}

	/* The pilot's name with their country's flag after it. */
	.pilot-line {
		grid-area: pilot;
		display: flex;
		align-items: center;
		gap: var(--space-2);
		min-width: 0;
	}

	.pilot {
		position: relative;
		z-index: 1;
		justify-self: start;
		max-width: 100%;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--link);
		font-weight: 600;
	}

	.where {
		grid-area: where;
		display: flex;
		align-items: center;
		gap: 0.25rem;
		min-width: 0;
		color: var(--text);
	}

	.name {
		min-width: 0;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.sign {
		flex: none;
		display: inline-flex;
		color: var(--icon-quiet);
		font-size: 0.875em;
	}

	.day {
		grid-area: day;
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	/* The whole card opens the flight. */
	.open::after {
		content: '';
		position: absolute;
		inset: 0;
		border-radius: var(--radius-card);
	}

	.open:focus-visible {
		outline: none;
	}

	.open:focus-visible::after {
		outline: 0.1875rem solid var(--color-focus);
		outline-offset: -0.1875rem;
	}

	/* The airtime and the distance, large, side by side. */
	.numbers {
		grid-area: numbers;
		display: flex;
		flex-wrap: wrap;
		gap: 0 var(--space-5);
		margin-top: var(--space-2);
	}

	.number {
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

	.shape {
		display: inline-block;
		margin-right: 0.125rem;
		font-size: 0.75rem;
		vertical-align: 0.15em;
		color: var(--text-secondary);
	}

	.said {
		grid-area: said;
		margin-top: var(--space-1);
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	@media (hover: hover) {
		.flight:has(.open:hover) {
			background: var(--fill);
		}

		.pilot:hover {
			text-decoration: underline;
		}
	}
</style>
