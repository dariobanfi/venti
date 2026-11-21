<!--
	The air over a take-off through one day, as a picture, as the iPhone's
	`AirChart`: for every hour how high the thermals go, as a column in the
	thermal scale's colour of the climb, with the climb over it; a cloud on
	the column where cumulus caps them; the wind at the model's levels as
	arrows; the take-off's height as a dashed line, and the model's own
	ground as a band at the foot. Every figure is the core's; the layout is
	`air-chart.ts`.
-->
<script lang="ts">
	import type { OutlookDay } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import { airHours, dayScale, heightLines, yOf } from './air-chart';

	interface Props {
		day: OutlookDay;
		/** The take-off's height, metres above sea level, where its source gives one. */
		takeoffM: number | undefined;
	}

	let { day, takeoffM }: Props = $props();

	const text = format();

	/** The width kept for the heights' figures, the hours' row at the foot and the climbs' at the head. */
	const GUTTER = 44;
	const FOOT = 20;
	const HEAD = 28;

	let width = $state(0);
	let height = $state(0);

	const scale = $derived(dayScale(day, takeoffM));
	const hours = $derived(scale ? airHours(day, scale) : []);
	const plot = $derived({
		x: GUTTER,
		y: HEAD,
		w: Math.max(0, width - GUTTER),
		h: Math.max(0, height - HEAD - FOOT)
	});
	const slot = $derived(hours.length > 0 ? plot.w / hours.length : 0);
	const climbs = $derived(hours.some((hour) => hour.climbMs !== undefined));
	/** Room beside an arrow for its speed. */
	const roomy = $derived(slot >= 36);

	const y = (metres: number) => (scale ? plot.y + yOf(scale, metres, plot.h) : 0);
	const middle = (index: number) => plot.x + slot * (index + 0.5);

	const caption = $derived(
		'How high the thermals go hour by hour, in the colour of the climb to expect, with the ' +
			'climb in m/s over each and a cloud where cumulus caps them. The arrows are the wind at ' +
			'the model’s levels, in km/h. ' +
			(takeoffM === undefined ? '' : 'The dashed line is the take-off. ') +
			'The band at the foot is the model’s ground, which is smoother than the terrain. ' +
			'Heights in metres above sea level.'
	);
</script>

{#if scale && day.hours.length > 0}
	<figure class="air">
		<div class="picture" bind:clientWidth={width} bind:clientHeight={height}>
			{#if plot.w > 0 && plot.h > 0}
				<svg
					{width}
					{height}
					role="img"
					aria-label="The air through the day: {day.thermalLine || 'no estimate of the thermals'}"
					data-testid="air-chart"
				>
					{#if climbs}
						<text
							class="quiet"
							x={plot.x - 6}
							y={HEAD / 2}
							text-anchor="end"
							dominant-baseline="middle"
						>
							m/s
						</text>
					{/if}
					{#each heightLines(scale) as line (line)}
						<line class="rule" x1={plot.x} x2={plot.x + plot.w} y1={y(line)} y2={y(line)} />
						<text
							class="quiet"
							x={plot.x - 6}
							y={y(line)}
							text-anchor="end"
							dominant-baseline="middle"
						>
							{text.metres(line)}
						</text>
					{/each}
					{#if day.groundM !== undefined}
						<rect
							class="ground"
							x={plot.x}
							y={y(day.groundM)}
							width={plot.w}
							height={Math.max(0, plot.y + plot.h - y(day.groundM))}
						/>
						<line
							class="ground-edge"
							x1={plot.x}
							x2={plot.x + plot.w}
							y1={y(day.groundM)}
							y2={y(day.groundM)}
						/>
					{/if}
					{#each hours as hour, index (index)}
						{@const x = middle(index)}
						<text
							class="quiet"
							{x}
							y={plot.y + plot.h + FOOT / 2}
							text-anchor="middle"
							dominant-baseline="middle"
						>
							{hour.hour}
						</text>
						{#if hour.column && day.groundM !== undefined}
							<rect
								class="column t{hour.column.step}"
								x={x - slot * 0.31}
								y={y(hour.column.topM)}
								width={slot * 0.62}
								height={y(day.groundM) - y(hour.column.topM)}
								rx="3"
							/>
							{#if hour.column.cloud}
								<rect
									class="cloud"
									x={x - slot * 0.42}
									y={y(hour.column.topM) - 9}
									width={slot * 0.84}
									height="9"
									rx="4.5"
								/>
							{/if}
						{/if}
						{#if hour.climbMs !== undefined}
							<text class="climb" {x} y={HEAD / 2} text-anchor="middle" dominant-baseline="middle">
								{text.forecastClimb(hour.climbMs)}
							</text>
						{/if}
						{#each hour.winds as level (level.altitudeM)}
							{@const cx = x - (roomy ? 8 : 0)}
							{@const cy = y(level.altitudeM)}
							{#if level.wind.fromDeg !== undefined}
								<path
									class="arrow"
									d="M0 -7 L4.5 6 L0 3 L-4.5 6 Z"
									transform="translate({cx} {cy}) rotate({level.wind.fromDeg + 180})"
								/>
							{:else}
								<circle class="arrow" {cx} {cy} r="2.5" />
							{/if}
							{#if roomy}
								<text class="ink" x={cx + 8} y={cy} dominant-baseline="middle">
									{level.wind.label}
								</text>
							{/if}
						{/each}
					{/each}
					{#if takeoffM !== undefined}
						<line
							class="takeoff"
							x1={plot.x}
							x2={plot.x + plot.w}
							y1={y(takeoffM)}
							y2={y(takeoffM)}
						/>
						<text class="ink" x={plot.x + plot.w - 2} y={y(takeoffM) - 4} text-anchor="end">
							Take-off
						</text>
					{/if}
				</svg>
			{/if}
		</div>
		<figcaption>{caption}</figcaption>
	</figure>
{/if}

<style>
	.air {
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.picture {
		height: 17.5rem;
	}

	svg {
		display: block;
		overflow: visible;
	}

	text {
		font-size: var(--caption);
		font-variant-numeric: tabular-nums;
	}

	.quiet {
		fill: var(--text-secondary);
	}

	.ink {
		fill: var(--text);
	}

	.climb {
		fill: var(--text);
		font-weight: 600;
	}

	.rule {
		stroke: var(--color-border);
		stroke-width: 1;
	}

	.ground {
		fill: var(--color-surface-raised);
	}

	.ground-edge {
		stroke: var(--color-border-strong);
		stroke-width: 1.5;
	}

	.cloud {
		fill: var(--color-surface);
		stroke: var(--text-secondary);
		stroke-width: 1.5;
	}

	.arrow {
		fill: var(--text);
	}

	.takeoff {
		stroke: var(--text);
		stroke-width: 1.5;
		stroke-dasharray: 5 4;
	}

	.t1 {
		fill: var(--thermal-1);
	}

	.t2 {
		fill: var(--thermal-2);
	}

	.t3 {
		fill: var(--thermal-3);
	}

	.t4 {
		fill: var(--thermal-4);
	}

	.t5 {
		fill: var(--thermal-5);
	}

	.t6 {
		fill: var(--thermal-6);
	}

	figcaption {
		font-size: var(--caption);
		color: var(--text-secondary);
	}
</style>
