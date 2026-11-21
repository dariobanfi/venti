<!--
	Everything known about a flight, as lists: how it scores as a
	cross-country flight, which airspace it entered, the time spent in
	thermals and on glides, every thermal and every glide, both extremes of
	altitude and climb, the recorder, the exact meaning of each colour.

	Choosing a thermal, a glide or a stretch inside airspace marks the moment
	it began on the map and the profile. On a narrow screen all this opens as a sheet from the flight
	panel; a wide one has room to show it under the panel all the time.

	The words are those of the iPhone app's `FlightDetailsSheet.swift`.
-->
<script lang="ts">
	import { AIRSPACE_COLORS } from '#lib/airspace/layers.ts';
	import { checkCaveat, crossingLines, nothingEntered } from '#lib/airspace/words.ts';
	import type {
		FlightAirspace,
		FlightAnalysis,
		FlightScore,
		FlightView,
		GlideSummary,
		ThermalSummary
	} from '#lib/core/index.ts';
	import { compassPoint, format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import { CLIMB_BANDS } from '#lib/design/palette.ts';

	interface Props {
		flight: FlightView;
		/** What happened in the flight, once it has been worked out. */
		analysis: FlightAnalysis | undefined;
		/** The flight checked against the airspace the app has, once that is done. */
		airspace: FlightAirspace | undefined;
		/** Called with a position in the flight's track to mark. */
		onselect: (trackIndex: number) => void;
		/** The page of the place the flight took off from, where the site has a server to ask. */
		fromHere?: string;
		/** The name of the site it took off from, where one is known. */
		takeoffName?: string;
	}

	let { flight, analysis, airspace, onselect, fromHere, takeoffName }: Props = $props();

	const text = format();

	/** A duration with its share of the airtime: "0:07 h, 24%". */
	function share(seconds: number, analysis: FlightAnalysis): string {
		const time = `${text.hoursMinutes(seconds)} h`;
		return analysis.airtimeS > 0 ? `${time}, ${text.percent(seconds / analysis.airtimeS)}` : time;
	}

	function thermalDetail(thermal: ThermalSummary): string {
		const minutes = `${text.minutesSeconds(thermal.exit.seconds - thermal.entry.seconds)} min`;
		if (!thermal.wind) return minutes;
		const speed = text.kilometresPerHour(thermal.wind.speedMs);
		return `${minutes} · wind ${compassPoint(thermal.wind.fromDeg)} ${speed} km/h`;
	}

	function glideDetail(glide: GlideSummary): string {
		return glide.glideRatio === undefined
			? 'no height lost'
			: `glide ${text.glideRatio(glide.glideRatio)}`;
	}

	const summary = $derived([
		['Date', text.day(flight.startUnixS)],
		[
			'Take-off',
			takeoffName
				? `${text.time(flight.startUnixS)} · ${takeoffName}`
				: text.time(flight.startUnixS)
		],
		['Landing', text.time(flight.startUnixS + flight.durationS)],
		['Airtime', `${text.hoursMinutes(flight.durationS)} h`],
		['Track length', `${text.kilometres(flight.trackLengthM)} km`]
	]);

	/** What a route is called. The rules that tell them apart are the core's. */
	const ROUTES = {
		freeFlight: 'Free flight',
		freeTriangle: 'Free triangle',
		faiTriangle: 'FAI triangle'
	} as const;

	function scoreRows(score: FlightScore): string[][] {
		return [
			['Scored as', score.closed ? `${ROUTES[score.kind]}, closed` : ROUTES[score.kind]],
			['Distance', `${text.kilometres(score.distanceM)} km`],
			['Points', text.points(score.points)],
			[
				'Start to finish',
				score.closingM === undefined ? undefined : `${text.kilometres(score.closingM)} km`
			]
		].filter(present);
	}

	/** The analysis, if the recording shows the glider flying at all. */
	const flown = $derived(analysis?.takeoff ? analysis : undefined);

	const extremes = $derived([
		['Highest', `${text.metres(flight.maxAltitudeM)} m`],
		['Lowest', `${text.metres(flight.minAltitudeM)} m`],
		['Best climb', `${text.climb(flight.maxClimbMs)} m/s`],
		['Strongest sink', `${text.climb(flight.maxSinkMs)} m/s`]
	]);

	const recording = $derived(
		[
			['Pilot', flight.pilot],
			['Glider', flight.glider],
			['Recorder', flight.recorder],
			['Positions', text.count(flight.track.length)],
			['Unreadable lines', flight.skippedLines > 0 ? text.count(flight.skippedLines) : undefined]
		].filter(present)
	);

	function present(row: (string | undefined)[]): row is [string, string] {
		return Boolean(row[1]);
	}
</script>

{#snippet facts(rows: string[][])}
	<dl class="card numeric">
		{#each rows as [name, value] (name)}
			<div class="row">
				<dt>{name}</dt>
				<dd>{value}</dd>
			</div>
		{/each}
	</dl>
{/snippet}

<!-- A stretch of the flight that can be chosen: when, and four things about it. -->
{#snippet stretch(
	trackIndex: number,
	seconds: number,
	detail: string,
	amount: string,
	rate: string
)}
	<li>
		<button class="row choice numeric" type="button" onclick={() => onselect(trackIndex)}>
			<span class="pair">
				<span>{text.time(flight.startUnixS + seconds)}</span>
				<span class="lesser">{detail}</span>
			</span>
			<span class="pair trailing">
				<span>{amount}</span>
				<span class="lesser">{rate}</span>
			</span>
		</button>
	</li>
{/snippet}

<div class="details" role="group" aria-label="Flight details">
	<section>
		<h3>Flight</h3>
		{@render facts(summary)}
		{#if fromHere}
			<div class="card from-here">
				<a class="row link" href={fromHere}>Flights from this take-off</a>
			</div>
		{/if}
	</section>

	{#if analysis?.score}
		{@const score = analysis.score}
		<section>
			<h3>Cross-country</h3>
			{@render facts(scoreRows(score))}
			<p class="footnote">
				Worked out by Venti from the World XContest's published rules, at
				{text.number(score.pointsPerKm)} points per kilometre for this kind of route. The dashed line
				on the map is the scored route. A league's own result can differ a little.
			</p>
		</section>
	{/if}

	{#if airspace}
		{@const nothing = nothingEntered(airspace)}
		<section>
			<h3 id="airspace-entered">Airspace</h3>
			{#if airspace.crossings.length > 0}
				<ul class="card" aria-labelledby="airspace-entered">
					{#each airspace.crossings as crossing (`${crossing.id} ${crossing.entry.trackIndex}`)}
						{@const lines = crossingLines(crossing, flight.startUnixS, text)}
						<li>
							<button
								class="row choice crossing"
								type="button"
								onclick={() => onselect(crossing.entry.trackIndex)}
							>
								<!-- The colour the airspace has on the map: to avoid, or to mind. -->
								<span class="sign" style:color={AIRSPACE_COLORS[crossing.policy]}>
									<Icon name={crossing.uncertain ? 'question' : 'warning'} />
								</span>
								<span>{lines.name}</span>
								<span class="numeric">{lines.duration}</span>
								<span class="lesser more">{lines.airspace}</span>
								<span class="lesser more numeric">{lines.stretch}</span>
								{#if crossing.uncertain}
									<span class="doubt more">{crossing.uncertain}</span>
								{/if}
							</button>
						</li>
					{/each}
				</ul>
			{/if}
			{#if nothing}
				<p class="card nothing">{nothing}</p>
			{/if}
			<p class="footnote">{checkCaveat(airspace.cover)}</p>
		</section>
	{/if}

	{#if flown}
		<section>
			<h3>In the air</h3>
			{@render facts([
				['In thermals', share(flown.thermallingS, flown)],
				['On glides', share(flown.glidingS, flown)],
				['Gained in thermals', `${text.metres(flown.heightGainedM)} m`]
			])}
			{#if !flown.landing}
				<p class="footnote">The recording ends in the air.</p>
			{/if}
		</section>

		{#if flown.thermals.length > 0}
			<section>
				<h3 id="thermals">Thermals</h3>
				<ul class="card" aria-labelledby="thermals">
					{#each flown.thermals as thermal (thermal.entry.trackIndex)}
						{@render stretch(
							thermal.entry.trackIndex,
							thermal.entry.seconds,
							thermalDetail(thermal),
							`${text.signedMetres(thermal.gainM)} m`,
							`${text.climb(thermal.climbMs)} m/s`
						)}
					{/each}
				</ul>
			</section>
		{/if}

		{#if flown.glides.length > 0}
			<section>
				<h3 id="glides">Glides</h3>
				<ul class="card" aria-labelledby="glides">
					{#each flown.glides as glide (glide.start.trackIndex)}
						{@render stretch(
							glide.start.trackIndex,
							glide.start.seconds,
							`${text.kilometresPerHour(glide.speedMs)} km/h`,
							`${text.kilometres(glide.distanceM)} km`,
							glideDetail(glide)
						)}
					{/each}
				</ul>
			</section>
		{/if}
	{/if}

	<section>
		<h3>Altitude and climb</h3>
		{@render facts(extremes)}
	</section>

	<section>
		<h3>Track colours</h3>
		<ul class="card">
			<!-- Strongest climb first, as the eye reads a scale: up is up. -->
			{#each CLIMB_BANDS.toReversed() as band (band.color)}
				<li class="row colour">
					<span class="swatch" style:background={band.color}></span>
					{band.label}
				</li>
			{/each}
		</ul>
		<p class="footnote">
			Climb and sink along the track are averaged over ten seconds. Best climb and strongest sink
			are averaged over half a minute.
		</p>
	</section>

	<section>
		<h3>Recording</h3>
		{@render facts(recording)}
	</section>
</div>

<style>
	.details {
		display: flex;
		flex-direction: column;
		gap: 1.5rem;
	}

	.from-here {
		margin-top: 0.75rem;
	}

	.link {
		color: var(--link);
	}

	@media (hover: hover) {
		.link:hover {
			background: var(--fill);
		}
	}

	.link:focus-visible {
		outline-offset: -0.1875rem;
		border-radius: var(--radius-card);
	}

	h3 {
		padding: 0 1rem 0.375rem;
		font-size: var(--subheadline);
		font-weight: 600;
		color: var(--text-secondary);
	}

	.card {
		background: var(--surface);
		border-radius: var(--radius-card);
		overflow: hidden;
	}

	.row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		min-height: var(--target);
		padding: 0.5rem 1rem;
	}

	/* A hairline between rows, starting where the text does. */
	.row + .row,
	li + li {
		box-shadow: inset 0 0.5px 0 var(--separator);
	}

	.colour {
		justify-content: flex-start;
		gap: 0.75rem;
	}

	dd {
		color: var(--text-secondary);
		text-align: right;
		overflow-wrap: anywhere;
	}

	.choice {
		width: 100%;
		text-align: left;
	}

	@media (hover: hover) {
		.choice:hover {
			background: var(--fill);
		}
	}

	.choice:active {
		background: var(--fill-strong);
	}

	.choice:focus-visible {
		outline-offset: -0.1875rem;
		border-radius: 0.5rem;
	}

	.pair {
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
		min-width: 0;
	}

	.trailing {
		flex: none;
		align-items: flex-end;
	}

	/* A stretch inside airspace: its sign, the airspace's name and how long
	   on the first line, and under them, at the full width, what it is, when
	   and how high, and what is in doubt. */
	.crossing {
		display: grid;
		grid-template-columns: auto minmax(0, 1fr) auto;
		gap: 0.125rem 0.75rem;
		align-items: start;
	}

	.crossing .more {
		grid-column: 2 / -1;
	}

	.sign {
		grid-row: 1 / span 2;
	}

	.doubt {
		font-size: var(--subheadline);
	}

	.nothing {
		padding: 0.75rem 1rem;
		color: var(--text-secondary);
	}

	.lesser {
		font-size: var(--subheadline);
		color: var(--text-secondary);
	}

	.swatch {
		flex: none;
		width: 1.375rem;
		height: 0.375rem;
		border-radius: 999px;
	}

	.footnote {
		padding: 0.5rem 1rem 0;
		font-size: var(--footnote);
		color: var(--text-secondary);
	}
</style>
