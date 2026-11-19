<!--
	What a station measured over the last 24 hours: the mean wind as a line
	with the span up to the gusts above it, and below it, in a small chart of
	its own, where the wind came from. Degrees and km/h have nothing to do
	with each other, so they never share a chart with two scales.

	Moving a finger or the mouse along either chart reads out one reading in
	the line above them. To the keyboard and to a screen reader the upper
	chart is a slider over the readings.
-->
<script lang="ts">
	import type { WindReading } from '#lib/core/index.ts';
	import { roundTicks, scale, timeTicks } from '#lib/flight/chart.ts';
	import { compassPoint, format } from '#lib/design/format.ts';
	import {
		caption,
		linePath,
		nearestSample,
		runs,
		samples as inChartUnits,
		spanPath,
		speedCeiling
	} from './history';

	let { readings }: { readings: WindReading[] } = $props();

	const text = format();

	/** The size of each plot in pixels, as the browser laid it out. */
	let width = $state(0);
	let speedHeight = $state(0);
	let directionHeight = $state(0);

	/** The position of the reading under the cursor, if any. */
	let marked = $state<number>();
	/** Whether the mark is where a mouse hovers, to be dropped when the mouse leaves. */
	let hovered = false;

	const samples = $derived(inChartUnits(readings));
	const start = $derived(samples[0].timeUnixS);
	const duration = $derived(Math.max(samples.at(-1)!.timeUnixS - start, 1));
	const ceiling = $derived(speedCeiling(samples));

	const x = $derived(scale([start, start + duration], [0, width]));
	const ySpeed = $derived(scale([0, ceiling], [speedHeight, 0]));
	const yDirection = $derived(scale([0, 360], [directionHeight, 0]));

	const speeds = $derived(roundTicks(0, ceiling));
	const times = $derived(
		timeTicks(duration, text.secondsIntoDay(start), 5).map((seconds) => start + seconds)
	);
	const drawn = $derived(runs(samples, x, ySpeed));
	const sample = $derived(marked === undefined ? undefined : samples[marked]);

	const COMPASS = [0, 90, 180, 270, 360];

	/** Marks the reading that lies under a place on a plot. */
	function mark(event: PointerEvent) {
		const plot = event.currentTarget as HTMLElement;
		const along = (event.clientX - plot.getBoundingClientRect().left) / Math.max(width, 1);
		marked = nearestSample(samples, start + Math.min(Math.max(along, 0), 1) * duration);
	}

	function pressed(event: PointerEvent) {
		// Follow the finger even when it strays off the chart.
		(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
		hovered = false;
		mark(event);
	}

	function moved(event: PointerEvent) {
		const plot = event.currentTarget as HTMLElement;
		if (plot.hasPointerCapture(event.pointerId)) {
			mark(event);
		} else if (event.pointerType === 'mouse' && (event.movementX || event.movementY)) {
			// A mouse reads the chart by passing over it. The browser also
			// reports a mouse that lies still when the page moves under it;
			// that must not take the mark from the keyboard.
			hovered = true;
			mark(event);
		}
	}

	/** A mouse that leaves lets go of what it was reading, and of nothing else. */
	function left() {
		if (hovered) marked = undefined;
		hovered = false;
	}

	/** How many readings each key moves the cursor. Stations report every ten minutes. */
	const STEPS: Record<string, number> = {
		ArrowRight: 1,
		ArrowUp: 1,
		ArrowLeft: -1,
		ArrowDown: -1,
		PageUp: 6,
		PageDown: -6
	};

	function keyed(event: KeyboardEvent) {
		const last = samples.length - 1;
		let to: number;
		if (event.key in STEPS) {
			// The first press marks the newest reading rather than stepping past it.
			to = marked === undefined ? last : marked + STEPS[event.key];
		} else if (event.key === 'Home') to = 0;
		else if (event.key === 'End') to = last;
		else if (event.key === 'Escape' && marked !== undefined) {
			marked = undefined;
			event.preventDefault();
			return;
		} else return;
		event.preventDefault();
		hovered = false;
		marked = Math.min(Math.max(to, 0), last);
	}

	const said = $derived(
		sample
			? caption(sample, text.time, compassPoint)
			: 'Wind and gusts in km/h, and where the wind came from.'
	);
</script>

{#snippet timeGrid(height: number)}
	{#each times as time (time)}
		<line class="grid dashed" x1={x(time)} x2={x(time)} y1="0" y2={height} />
	{/each}
{/snippet}

{#snippet rule(height: number)}
	{#if sample}
		<line class="rule" x1={x(sample.timeUnixS)} x2={x(sample.timeUnixS)} y1="0" y2={height} />
	{/if}
{/snippet}

<div class="charts">
	<p class="caption numeric">{said}</p>

	<div class="chart speed">
		<div
			class="plot"
			bind:clientWidth={width}
			bind:clientHeight={speedHeight}
			role="slider"
			tabindex="0"
			aria-label="Wind and gusts over the last 24 hours, in kilometres per hour"
			aria-orientation="horizontal"
			aria-valuemin={0}
			aria-valuemax={samples.length - 1}
			aria-valuenow={marked ?? samples.length - 1}
			aria-valuetext={sample ? said : 'Nothing marked.'}
			onpointerdown={pressed}
			onpointermove={moved}
			onpointerleave={left}
			onkeydown={keyed}
		>
			<svg {width} height={speedHeight} aria-hidden="true" focusable="false">
				{#each speeds as speed (speed)}
					<line class="grid" x1="0" x2={width} y1={ySpeed(speed)} y2={ySpeed(speed)} />
				{/each}
				<line class="grid" x1="0" x2={width} y1={speedHeight} y2={speedHeight} />
				{@render timeGrid(speedHeight)}
				{#if width > 0}
					{#each drawn as run, index (index)}
						<path class="span" d={spanPath(run)} />
						<path class="line" d={linePath(run)} />
					{/each}
				{/if}
				{@render rule(speedHeight)}
				{#if sample?.wind !== undefined}
					<circle class="mark" cx={x(sample.timeUnixS)} cy={ySpeed(sample.wind)} r="4" />
				{/if}
			</svg>
		</div>
		<!-- The labels are for the eye. The slider says the same in words. -->
		<div class="values numeric" aria-hidden="true">
			<span class="widest">{text.count(ceiling)}</span>
			{#each speeds as speed (speed)}
				<span style:top="{ySpeed(speed)}px">{text.count(speed)}</span>
			{/each}
		</div>
	</div>

	<div class="chart direction">
		<div
			class="plot"
			bind:clientHeight={directionHeight}
			role="img"
			aria-label="Wind direction over the last 24 hours"
			onpointerdown={pressed}
			onpointermove={moved}
			onpointerleave={left}
		>
			<svg {width} height={directionHeight} aria-hidden="true" focusable="false">
				{#each COMPASS as degrees (degrees)}
					<line class="grid" x1="0" x2={width} y1={yDirection(degrees)} y2={yDirection(degrees)} />
				{/each}
				{@render timeGrid(directionHeight)}
				{#if width > 0}
					{#each samples as reading (reading.timeUnixS)}
						{#if reading.from !== undefined}
							<circle
								class="point"
								cx={x(reading.timeUnixS)}
								cy={yDirection(reading.from)}
								r="1.75"
							/>
						{/if}
					{/each}
				{/if}
				{@render rule(directionHeight)}
			</svg>
		</div>
		<div class="values" aria-hidden="true">
			<span class="widest">{text.count(ceiling)}</span>
			{#each COMPASS as degrees (degrees)}
				<span style:top="{yDirection(degrees)}px">{compassPoint(degrees)}</span>
			{/each}
		</div>
		<div class="times numeric" aria-hidden="true">
			{#each times as time (time)}
				<span style:left="{x(time)}px">{text.time(time)}</span>
			{/each}
		</div>
	</div>
</div>

<style>
	.charts {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		font-size: var(--caption);
		color: var(--text-secondary);
	}

	.caption {
		font-size: var(--footnote);
		/* Room for two lines whatever it says, so the charts do not jump
		   under the cursor. */
		min-height: 2.7em;
	}

	.chart {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		column-gap: 0.5rem;
		row-gap: 0.25rem;
	}

	.speed .plot {
		height: 10.5rem;
	}

	.direction .plot {
		height: 6rem;
	}

	.plot {
		position: relative;
		cursor: crosshair;
		/* A finger moving sideways reads the chart; up and down scrolls the page. */
		touch-action: pan-y;
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

	.span {
		fill: var(--gusts);
	}

	.line {
		fill: none;
		stroke: var(--wind);
		stroke-width: 2;
		stroke-linecap: round;
		stroke-linejoin: round;
	}

	.point,
	.mark {
		fill: var(--wind);
	}

	.rule {
		stroke: var(--text);
		stroke-opacity: 0.45;
		stroke-width: 1;
	}

	.values {
		position: relative;
	}

	/* Holds the column as wide as its widest label, the same in both charts
	   so that their plots line up; the labels themselves are placed over it. */
	.widest {
		visibility: hidden;
	}

	.values span:not(.widest) {
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
		/* Centred under its line: the last one would otherwise run out of the chart. */
		transform: translateX(-50%);
		white-space: nowrap;
	}
</style>
