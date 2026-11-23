<!--
	The panel beside or over the flight map: when the flight was, its headline
	numbers, and its altitude profile.

	Dragging along the profile moves a marker along the track, and the numbers
	change to that moment. The marker stays where the finger left it, so the
	pilot can look from the chart to the map; the button in the corner returns
	to the totals.
-->
<script lang="ts">
	import PilotFace from '#lib/community/PilotFace.svelte';
	import type { FlightScore, FlightView, GroundUnderFlight } from '#lib/core/index.ts';
	import { flightFormat } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import Metric from '#lib/design/Metric.svelte';
	import type { Beside } from './beside';
	import FlightChart from './FlightChart.svelte';
	import { heightAboveGround } from './ground';

	interface Props {
		flight: FlightView;
		/** The ground under the flight, where the app has it. Empty without it. */
		ground?: readonly GroundUnderFlight[];
		/** The position in the track the pilot is inspecting, if any. */
		cursor: number | undefined;
		/** Whether the marker is moving through the flight by itself. */
		playing: boolean;
		/** Opens everything else known about the flight, where that is not on screen already. */
		ondetails?: () => void;
		/** Whose flight it is, for another pilot's: the name, and the way to that pilot's flights. */
		whose?: { name: string; href: string; picture?: string };
		/** A second flight laid beside this one. */
		beside?: Beside;
		/** Opens the two flights side by side in numbers. */
		onbeside?: () => void;
		/** How the flight scores, once worked out: its distance is the flight's one number. */
		score?: FlightScore;
		/** Hands the flight to a league: for a flight of the pilot's own. */
		onleague?: () => void;
		/** Whether the altitude profile is in the panel: not where the screen has it under the map. */
		chart?: boolean;
	}

	let {
		flight,
		ground = [],
		cursor = $bindable(),
		playing = $bindable(),
		ondetails,
		whose,
		beside,
		onbeside,
		score,
		onleague,
		chart = true
	}: Props = $props();

	/** What kind of flight it was, by its score, and the day: "FAI triangle, Sat, 4 Jul 2026". */
	const kindAndDay = $derived.by(() => {
		const day = text.date(flight.startUnixS);
		if (!score) return day;
		const kind = {
			freeFlight: 'Free flight',
			freeTriangle: score.closed ? 'Closed triangle' : 'Triangle',
			faiTriangle: score.closed ? 'Closed FAI triangle' : 'FAI triangle'
		}[score.kind];
		return `${kind}, ${day}`;
	});

	// The clock and the date of the place it was flown.
	const text = $derived(flightFormat(flight));
	const moment = $derived(cursor === undefined ? undefined : flight.track[cursor]);
	/** How high over the ground the glider was at the marked moment, where the ground is known. */
	const above = $derived(moment ? heightAboveGround(moment.seconds, ground) : undefined);
	const times = $derived(
		`${text.time(flight.startUnixS)} – ${text.time(flight.startUnixS + flight.durationS)}`
	);
	/**
	 * The time of day at the marked moment, with a region's "AM" or "PM" apart
	 * from it. Set small, as a unit is, it leaves the time as narrow as the
	 * three readings beside it: "10:52 AM" in one size runs into the next one.
	 */
	const clock = $derived.by(() => {
		if (!moment) return undefined;
		const [value = '', period] = text.time(flight.startUnixS + moment.seconds).split(/\s/u);
		return { value, period };
	});
</script>

<div class="panel">
	<header>
		<div class="when">
			{#if whose}
				<a class="whose" href={whose.href}>
					<PilotFace picture={whose.picture} name={whose.name} />
					<span class="whose-name">{whose.name}</span>
				</a>
			{/if}
			<!-- The flight in one number, as the logbook's list has it: the
			     scored distance, or the track's length until it is scored. -->
			<p
				class="hero numeric"
				aria-label="{score ? 'Distance' : 'Track'}: {text.kilometres(
					score?.distanceM ?? flight.trackLengthM
				)} kilometres"
			>
				{text.kilometres(score?.distanceM ?? flight.trackLengthM)}<small>km</small>
			</p>
			<h1>{kindAndDay}</h1>
			<p>{flight.glider ? `${times} · ${flight.glider}` : times}</p>
		</div>
		{#if moment}
			<button
				class="corner quiet"
				type="button"
				aria-label="Show flight totals"
				title="Show flight totals"
				onclick={() => {
					playing = false;
					cursor = undefined;
				}}
			>
				<span class="disc"><Icon name="close" /></span>
			</button>
		{/if}
		<button
			class="corner"
			type="button"
			aria-label={playing ? 'Pause' : 'Play the flight'}
			title={playing ? 'Pause' : 'Play the flight'}
			onclick={() => (playing = !playing)}
		>
			<Icon name={playing ? 'pause' : 'play'} />
		</button>
		{#if ondetails}
			<button
				class="corner"
				type="button"
				aria-label="Flight details"
				title="Flight details"
				onclick={ondetails}
			>
				<Icon name="info" />
			</button>
		{/if}
	</header>

	<!-- Announced when it changes, so a screen reader follows the cursor too,
	     but not fifteen times a second while the flight plays. -->
	<dl
		class="numbers"
		aria-live={playing ? 'off' : 'polite'}
		aria-label={moment ? 'At the marked moment' : 'The whole flight'}
	>
		{#if moment && clock}
			<Metric caption="Time" value={clock.value} unit={clock.period} />
			<!-- How high over the ground the glider was, where the ground is
			     known; else how long it had been in the air. -->
			{#if above !== undefined}
				<Metric caption="Over ground" value={text.metres(Math.max(above, 0))} unit="m" />
			{:else}
				<Metric caption="In the air" value={text.hoursMinutes(moment.seconds)} unit="h" />
			{/if}
			<Metric caption="Altitude" value={text.metres(moment.altitudeM)} unit="m" />
			<Metric caption="Climb" value={text.climb(moment.climbMs)} unit="m/s" />
		{:else}
			<Metric caption="Airtime" value={text.hoursMinutes(flight.durationS)} unit="h" />
			<Metric caption="Highest" value={text.metres(flight.maxAltitudeM)} unit="m" />
			<Metric caption="Best climb" value={text.climb(flight.maxClimbMs)} unit="m/s" />
		{/if}
	</dl>

	{#if chart}
		<FlightChart {flight} {ground} bind:cursor {playing} {beside} {onbeside} />
	{/if}
	{#if onleague}
		<button class="button-prominent league" type="button" onclick={onleague}>
			Send to a league
		</button>
	{/if}
</div>

<style>
	.panel {
		display: flex;
		flex-direction: column;
		gap: 0.875rem;
	}

	header {
		display: flex;
		align-items: center;
		gap: 0.25rem;
		/* The buttons are taller than the two lines of text beside them. Let
		   them hang into the panel's padding rather than push the text down. */
		margin: -0.25rem -0.5rem -0.25rem 0;
	}

	.when {
		flex: 1;
		min-width: 0;
	}

	/* The pack's hero style, in rem so that the reader's text size counts. */
	.when .hero {
		font-size: 2.25rem;
		white-space: nowrap;
		font-style: italic;
		font-weight: 900;
		line-height: 1.05;
		letter-spacing: -0.01em;
		color: var(--text);
	}

	.hero small {
		margin-left: 0.15em;
		font-size: 0.6em;
	}

	h1 {
		font-size: var(--body);
		font-weight: 500;
	}

	.league {
		width: 100%;
	}

	/* Whose flight it is, for another pilot's: over the day, and the way to
	   that pilot's other flights. Underlined, in the text's own colour: the
	   accent does not hold its contrast on glass. */
	.whose {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		font-size: var(--subheadline);
		font-weight: 600;
	}

	.whose-name {
		min-width: 0;
		text-decoration: underline;
		text-decoration-thickness: 0.0625em;
		text-underline-offset: 0.15em;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.when p {
		font-size: var(--subheadline);
		color: var(--metric-secondary, var(--text-secondary));
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	/* An icon button in the panel's corner, with a full-size touch target. */
	.corner {
		flex: none;
		display: grid;
		place-items: center;
		width: var(--target);
		height: var(--target);
		border-radius: 50%;
		color: var(--link);
		font-size: 1.125rem;
	}

	.quiet {
		color: var(--metric-secondary, var(--text-secondary));
	}

	.disc {
		display: grid;
		place-items: center;
		width: 1.75rem;
		height: 1.75rem;
		border-radius: 50%;
		background: var(--fill-strong);
		font-size: 0.75rem;
	}

	@media (hover: hover) {
		.corner:hover {
			background: var(--fill);
		}
	}

	.numbers {
		display: flex;
		gap: 0.5rem;
	}
</style>
