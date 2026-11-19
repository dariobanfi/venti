<!--
	One flight in the list: its shape, when it was, and how long and far. The
	row answers "which flight" without being opened.

	A wide screen has room for two more numbers the core already knows, and
	for a button to delete the flight. On a narrow one, deleting is in the
	flight's own screen.
-->
<script lang="ts">
	import { flightHref } from '#lib/app/routes.ts';
	import type { LogbookEntry } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import TrackThumbnail from './TrackThumbnail.svelte';

	interface Props {
		entry: LogbookEntry;
		ondelete: (entry: LogbookEntry) => void;
	}

	let { entry, ondelete }: Props = $props();

	const text = format();
	const detail = $derived(
		entry.glider ? `${text.time(entry.startUnixS)} · ${entry.glider}` : text.time(entry.startUnixS)
	);
</script>

<li>
	<a class="flight" href={flightHref(entry.id)}>
		<TrackThumbnail outline={entry.outline} />
		<span class="when">
			<span class="day">{text.dayInYear(entry.startUnixS)}</span>
			<span class="detail">{detail}</span>
		</span>
		<span class="figures numeric">
			<span class="figure airtime">
				<span class="visually-hidden">Airtime</span>
				{text.hoursMinutes(entry.durationS)} h
			</span>
			<!-- The scored cross-country distance; a triangle is marked. -->
			<span class="figure distance">
				<span class="visually-hidden">Distance</span>
				{#if entry.score && entry.score.kind !== 'freeFlight'}
					<span
						class="shape"
						role="img"
						aria-label={entry.score.kind === 'faiTriangle' ? 'FAI triangle' : 'Triangle'}
					>
						<Icon name="triangle" />
					</span>
				{/if}
				{text.kilometres(entry.score?.distanceM ?? entry.trackLengthM)} km
			</span>
			<span class="figure extra">
				<span class="visually-hidden">Highest</span>
				{text.metres(entry.maxAltitudeM)} m
			</span>
			<span class="figure extra">
				<span class="visually-hidden">Best climb</span>
				{text.climb(entry.maxClimbMs)} m/s
			</span>
		</span>
		<span class="chevron"><Icon name="forward" /></span>
	</a>
	<button
		class="delete"
		type="button"
		aria-label="Delete the flight of {text.day(entry.startUnixS)}"
		title="Delete flight"
		onclick={() => ondelete(entry)}
	>
		<Icon name="trash" />
	</button>
</li>

<style>
	li {
		position: relative;
		display: flex;
		align-items: center;
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

	.flight {
		flex: 1;
		min-width: 0;
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.625rem 0.75rem 0.625rem 1rem;
	}

	.when {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
	}

	.day {
		font-weight: 600;
	}

	.detail {
		font-size: var(--subheadline);
		color: var(--text-secondary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	/* Narrow: airtime over distance, at the row's right edge. */
	.figures {
		display: flex;
		flex-direction: column;
		align-items: flex-end;
		gap: 0.125rem;
		white-space: nowrap;
	}

	.airtime {
		font-weight: 600;
	}

	.distance {
		font-size: var(--subheadline);
		color: var(--text-secondary);
	}

	.shape {
		display: inline-block;
		font-size: 0.5rem;
		vertical-align: 0.05em;
	}

	.extra,
	.delete {
		display: none;
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

	.flight:active {
		background: var(--fill-strong);
	}

	/* The focus ring would be cut off by the card's rounded corners. */
	.flight:focus-visible {
		outline-offset: -0.1875rem;
		border-radius: 0.5rem;
	}

	/* Wide: the numbers in columns under the headings of `LogbookList`. */
	@media (min-width: 56rem) {
		.flight {
			padding-right: 0;
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

		.delete {
			display: grid;
			place-items: center;
			width: var(--target);
			height: var(--target);
			margin: 0 0.5rem;
			border-radius: 50%;
			color: var(--text-secondary);
		}

		.delete:hover {
			background: var(--fill);
			color: var(--danger);
		}
	}

	/* Where there is a mouse, the delete button waits until the row is
	   pointed at. The keyboard finds it either way. */
	@media (min-width: 56rem) and (hover: hover) {
		.delete {
			opacity: 0;
		}

		li:hover .delete,
		.delete:focus-visible {
			opacity: 1;
		}
	}
</style>
