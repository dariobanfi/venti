<!--
	The flight's altitude over time, over the ground it was flown over where
	the app has that: the earth as a filled shape along the foot of the
	chart, and the room between it and the flight's line the height over the
	ground. Dragging along the chart sets the cursor: the moment of the
	flight the pilot is looking at. The cursor stays where the finger left it.

	To the keyboard and to a screen reader it is a slider over the flight's
	duration: the arrow keys move through the flight, Escape lets go of it.
-->
<script lang="ts">
	import type { FlightView, GroundUnderFlight } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import { altitudeRange, roundTicks, nearestIndex, scale, timeTicks } from './chart';
	import { groundRuns, heightAboveGround } from './ground';

	interface Props {
		flight: FlightView;
		/** The ground under the flight, in order of time. Empty without it. */
		ground?: readonly GroundUnderFlight[];
		/** The position in the track that is marked, if any. */
		cursor: number | undefined;
	}

	let { flight, ground = [], cursor = $bindable() }: Props = $props();

	const text = format();

	/** The size of the plot in pixels, as the browser laid it out. */
	let width = $state(0);
	let height = $state(0);

	const duration = $derived(Math.max(flight.durationS, 1));
	const [low, high] = $derived(altitudeRange(flight, ground));
	const x = $derived(scale([0, duration], [0, width]));
	const y = $derived(scale([low, high], [height, 0]));

	const altitudes = $derived(roundTicks(low, high));
	const times = $derived(timeTicks(duration, text.secondsIntoDay(flight.startUnixS)));

	/** The profile as the outline of the area under it, and as the line on top. */
	const line = $derived(
		flight.profile
			.map(
				(point, index) =>
					`${index ? 'L' : 'M'}${x(point.seconds).toFixed(1)},${y(point.altitudeM).toFixed(1)}`
			)
			.join('')
	);
	const area = $derived(line ? `${line}L${x(duration).toFixed(1)},${height}L0,${height}Z` : '');

	/**
	 * The ground under the glider, from the foot of the chart up to its
	 * height: one shape for each stretch it is known along.
	 */
	const earth = $derived(
		groundRuns(ground)
			.map((run) => {
				const top = run
					.map(
						(moment) =>
							`L${x(moment.seconds).toFixed(1)},${y(Math.min(Math.max(moment.elevationM, low), high)).toFixed(1)}`
					)
					.join('');
				const [from, to] = [x(run[0].seconds).toFixed(1), x(run.at(-1)!.seconds).toFixed(1)];
				return `M${from},${height}${top}L${to},${height}Z`;
			})
			.join('')
	);

	const marked = $derived(cursor === undefined ? undefined : flight.track[cursor]);
	/** How high over the ground the glider was at the marked moment, where that is known. */
	const above = $derived(marked ? heightAboveGround(marked.seconds, ground) : undefined);

	/** Marks the moment that lies under a place on the plot. */
	function mark(event: PointerEvent) {
		const plot = event.currentTarget as HTMLElement;
		const along = (event.clientX - plot.getBoundingClientRect().left) / Math.max(width, 1);
		cursor = nearestIndex(flight.track, Math.min(Math.max(along, 0), 1) * duration);
	}

	function pressed(event: PointerEvent) {
		// Follow the finger even when it strays off the chart.
		(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
		mark(event);
	}

	function moved(event: PointerEvent) {
		if ((event.currentTarget as HTMLElement).hasPointerCapture(event.pointerId)) mark(event);
	}

	/** How far through the flight each key moves the cursor, as a share of its duration. */
	const STEPS: Record<string, number> = {
		ArrowRight: 0.01,
		ArrowUp: 0.01,
		ArrowLeft: -0.01,
		ArrowDown: -0.01,
		PageUp: 0.1,
		PageDown: -0.1
	};

	function keyed(event: KeyboardEvent) {
		const from = marked?.seconds ?? 0;
		let to: number;
		if (event.key in STEPS) {
			// The first press marks the start of the flight rather than stepping past it.
			to = marked ? from + STEPS[event.key] * duration : 0;
		} else if (event.key === 'Home') to = 0;
		else if (event.key === 'End') to = duration;
		else if (event.key === 'Escape' && cursor !== undefined) {
			cursor = undefined;
			event.preventDefault();
			return;
		} else return;
		event.preventDefault();
		cursor = nearestIndex(flight.track, Math.min(Math.max(to, 0), duration));
	}

	/** The lowest and the highest ground under the flight, where the app has the ground. */
	const groundSpan = $derived(
		ground.length === 0
			? undefined
			: ground.reduce(
					([lowest, highest], moment) => [
						Math.min(lowest, moment.elevationM),
						Math.max(highest, moment.elevationM)
					],
					[Infinity, -Infinity]
				)
	);

	// What the picture shows, in words: the drawing itself says nothing to
	// a screen reader.
	const spoken = $derived(
		marked
			? [
					text.time(flight.startUnixS + marked.seconds),
					`${text.metres(marked.altitudeM)} metres`,
					...(above === undefined
						? []
						: [`${text.metres(Math.max(above, 0))} metres over the ground`]),
					`${text.climb(marked.climbMs)} metres per second`
				].join(', ')
			: [
					`From ${text.metres(flight.minAltitudeM)} to ${text.metres(flight.maxAltitudeM)} metres`,
					...(groundSpan
						? [
								`over ground from ${text.metres(groundSpan[0])} to ${text.metres(groundSpan[1])} metres`
							]
						: [])
				].join(', ') + '. Nothing marked.'
	);
</script>

<div class="chart">
	<div
		class="plot"
		bind:clientWidth={width}
		bind:clientHeight={height}
		role="slider"
		tabindex="0"
		aria-label="Altitude over time"
		aria-orientation="horizontal"
		aria-valuemin={0}
		aria-valuemax={duration}
		aria-valuenow={marked?.seconds ?? 0}
		aria-valuetext={spoken}
		onpointerdown={pressed}
		onpointermove={moved}
		onkeydown={keyed}
	>
		<svg {width} {height} aria-hidden="true" focusable="false">
			<defs>
				<linearGradient id="altitude-fill" x1="0" y1="0" x2="0" y2="1">
					<stop offset="0" stop-color="var(--accent)" stop-opacity="0.32" />
					<stop offset="1" stop-color="var(--accent)" stop-opacity="0.04" />
				</linearGradient>
			</defs>
			{#each altitudes as altitude (altitude)}
				<line class="grid" x1="0" x2={width} y1={y(altitude)} y2={y(altitude)} />
			{/each}
			{#each times as seconds (seconds)}
				<line class="grid dashed" x1={x(seconds)} x2={x(seconds)} y1="0" y2={height} />
			{/each}
			{#if width > 0}
				<path class="area" d={area} />
				<!-- Over the foot of the flight's own shape, and under its line. -->
				{#if earth}
					<path class="ground" d={earth} />
				{/if}
				<path class="line" d={line} />
			{/if}
			{#if marked}
				<line class="rule" x1={x(marked.seconds)} x2={x(marked.seconds)} y1="0" y2={height} />
				<circle class="mark" cx={x(marked.seconds)} cy={y(marked.altitudeM)} r="5" />
			{/if}
		</svg>
	</div>

	<!-- The labels are for the eye. The slider says the same in words. -->
	<div class="altitudes numeric" aria-hidden="true">
		<span class="widest">{text.metres(high)} m</span>
		{#each altitudes as altitude (altitude)}
			<span style:top="{y(altitude)}px">{text.metres(altitude)} m</span>
		{/each}
	</div>
	<div class="times numeric" aria-hidden="true">
		{#each times as seconds (seconds)}
			<span style:left="{x(seconds)}px">{text.time(flight.startUnixS + seconds)}</span>
		{/each}
	</div>
</div>

<style>
	.chart {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		grid-template-rows: minmax(0, 1fr) auto;
		column-gap: 0.5rem;
		row-gap: 0.25rem;
		height: 100%;
		font-size: var(--caption);
		color: var(--metric-secondary, var(--text-secondary));
	}

	.plot {
		position: relative;
		min-height: 0;
		cursor: crosshair;
		/* The drag belongs to the chart, not to scrolling the page. */
		touch-action: none;
		-webkit-user-select: none;
		user-select: none;
		border-radius: 0.25rem;
	}

	svg {
		position: absolute;
		inset: 0;
		display: block;
		overflow: visible;
	}

	.grid {
		stroke: var(--separator);
		stroke-width: 1;
	}

	.dashed {
		stroke-dasharray: 2 3;
	}

	.area {
		fill: url(#altitude-fill);
	}

	/* The ground: the colour of earth, the same in both appearances as on
	   the iPhone, solid enough to cover the foot of the flight's own shape. */
	.ground {
		fill: var(--ground);
	}

	.line {
		fill: none;
		stroke: var(--accent);
		stroke-width: 2;
		stroke-linecap: round;
		stroke-linejoin: round;
	}

	.rule {
		stroke: var(--text);
		stroke-opacity: 0.45;
		stroke-width: 1;
	}

	.mark {
		fill: var(--accent);
	}

	.altitudes {
		position: relative;
	}

	/* Holds the column as wide as its widest label; the labels themselves
	   are placed over it. */
	.widest {
		visibility: hidden;
	}

	.altitudes span:not(.widest) {
		position: absolute;
		left: 0;
		transform: translateY(-50%);
		white-space: nowrap;
	}

	.times {
		position: relative;
		height: 1.25em;
	}

	.times span {
		position: absolute;
		top: 0;
		white-space: nowrap;
	}
</style>
