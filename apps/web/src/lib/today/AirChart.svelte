<!--
	The air over a take-off through one day, as a picture, as the iPhone's
	`AirChart`: for every hour how high the thermals go, as a column in the
	thermal scale's colour of the climb, with the climb over it; a cloud on
	the column where cumulus caps them; the wind at the model's levels as
	arrows; the take-off's height as a dashed line, and the model's own
	ground as a band at the foot. Every figure is the core's; the layout is
	`air-chart.ts`.

	Where thermals stop by the model's temperatures, the hour's column has
	a band between the layer's two levels: hatched for a stable layer, solid
	for an inversion. An hour chosen says under the picture what the core
	says of it: what is much stronger near by, its layers in words, and the
	strongest wind and gust near by with where they are (`air-around.ts`).
-->
<script lang="ts">
	import type { OutlookDay } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import { clock, figures, hasDetail, spoken } from './air-around';
	import { airHours, dayScale, heightLines, yOf } from './air-chart';
	import NearbyLine from './NearbyLine.svelte';

	interface Props {
		day: OutlookDay;
		/** The take-off's height, metres above sea level, where its source gives one. */
		takeoffM: number | undefined;
		/** What the wind near by is and is not, the core's sentence (`SiteOutlook.nearbyNote`). */
		nearbyNote?: string;
		/** What the layers are and are not, the core's sentence (`SiteOutlook.layersNote`). */
		layersNote?: string;
		/** The heading over the notes on how the forecast is read, on a site's page: the core's. */
		notesTitle?: string;
	}

	let { day, takeoffM, nearbyNote = '', layersNote = '', notesTitle = '' }: Props = $props();

	/** Names the hatch of a stable layer for this picture alone. */
	const uid = $props.id();
	const hatch = `${uid}-stable`;

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

	/** Whether the day has a layer anywhere, and an hour with something to say of it. */
	const layered = $derived(day.hours.some((hour) => hour.layers.length > 0));
	const detailed = $derived(day.hours.some(hasDetail));
	/** The hour chosen, by its first moment; none until the reader chooses one. */
	let chosenAt = $state<number>();
	const chosen = $derived(day.hours.find((hour) => hour.timeUnixS === chosenAt && hasDetail(hour)));

	const caption = $derived(
		'How high the thermals go hour by hour, in the colour of the climb to expect, with the ' +
			'climb in m/s over each and a cloud where cumulus caps them. The arrows are the wind at ' +
			'the model’s levels, in km/h. ' +
			(takeoffM === undefined ? '' : 'The dashed line is the take-off. ') +
			'The band at the foot is the model’s ground, which is smoother than the terrain. ' +
			'Heights in metres above sea level.' +
			(layered ? ' A hatched band is a stable layer and a solid band an inversion.' : '') +
			(detailed ? ' Choose an hour for what the forecast says of the air around and above it.' : '')
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
					<defs>
						<pattern
							id={hatch}
							width="6"
							height="6"
							patternUnits="userSpaceOnUse"
							patternTransform="rotate(45)"
						>
							<line class="hatch" x1="0" y1="0" x2="0" y2="6" />
						</pattern>
					</defs>
					{#each day.hours as hour, index (hour.timeUnixS)}
						{#if hour === chosen}
							<rect class="chosen" x={plot.x + slot * index} y="0" width={slot} {height} rx="4" />
						{/if}
					{/each}
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
						<!-- Under the arrows and over the thermals' column, across the hour's whole slot. -->
						{#each hour.bands as band (band.bottomM)}
							<rect
								class="layer"
								class:inversion={band.inversion}
								x={x - slot / 2}
								y={y(band.topM)}
								width={slot}
								height={y(band.bottomM) - y(band.topM)}
								fill={band.inversion ? undefined : `url(#${hatch})`}
							/>
						{/each}
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
				{#if detailed}
					<!-- An hour is chosen by its column: a button over each, for a finger and a keyboard. -->
					<div class="choose" style:left="{plot.x}px">
						{#each day.hours as hour (hour.timeUnixS)}
							{#if hasDetail(hour)}
								<button
									type="button"
									aria-pressed={hour === chosen}
									aria-label="{clock(hour)}: {spoken(hour, text.metres)}"
									onclick={() => (chosenAt = hour === chosen ? undefined : hour.timeUnixS)}
								></button>
							{:else}
								<span></span>
							{/if}
						{/each}
					</div>
				{/if}
			{/if}
		</div>
		{#if chosen}
			<div
				class="detail"
				role="group"
				aria-label="The air at {clock(chosen)}"
				data-testid="air-hour"
			>
				<p class="clock">{clock(chosen)}</p>
				<NearbyLine words={chosen.nearby?.line ?? ''} small />
				{#if chosen.layers.length > 0}
					<ul>
						{#each chosen.layers as layer (layer.bottomM)}
							<li>
								<!-- The band as the picture draws it, so that the words name their mark. -->
								<svg class="swatch" viewBox="0 0 16 12" aria-hidden="true">
									<rect
										class="layer"
										class:inversion={layer.inversion}
										width="16"
										height="12"
										fill={layer.inversion ? undefined : `url(#${hatch})`}
									/>
								</svg>
								{layer.words}
							</li>
						{/each}
					</ul>
				{/if}
				<dl>
					{#each figures(chosen.nearby, text.metres) as each (each.name)}
						<div>
							<dt>{each.name}</dt>
							<dd>{each.figure}</dd>
						</div>
					{/each}
				</dl>
			</div>
		{/if}
		<figcaption>
			{#if notesTitle}
				<p class="notes-title">{notesTitle}</p>
			{/if}
			<p>{caption}</p>
			{#if layered && layersNote}
				<p>{layersNote}</p>
			{/if}
			{#if detailed && nearbyNote}
				<p>{nearbyNote}</p>
			{/if}
		</figcaption>
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
		position: relative;
		height: 17.5rem;
	}

	/* The hours' buttons: as wide as the hours' columns, over the whole picture. */
	.choose {
		position: absolute;
		inset: 0;
		display: grid;
		grid-auto-flow: column;
		grid-auto-columns: minmax(0, 1fr);
	}

	.choose button {
		border-radius: var(--radius-s);
		cursor: pointer;
	}

	.choose button:focus-visible {
		outline-offset: -0.125rem;
	}

	.chosen {
		fill: var(--fill);
	}

	/* A layer in which thermals stop. The pack has no token for a band:
	   `text-muted` stands in, hatched for a stable layer, solid for an
	   inversion. */
	.layer.inversion {
		fill: var(--color-text-muted);
	}

	.hatch {
		stroke: var(--color-text-muted);
		stroke-width: 1;
	}

	.detail {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		font-size: var(--caption);
		color: var(--text-secondary);
	}

	.clock {
		font-size: var(--subheadline);
		font-weight: 600;
		font-variant-numeric: tabular-nums;
		color: var(--text);
	}

	.detail ul {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}

	.detail li {
		display: flex;
		align-items: flex-start;
		gap: var(--space-2);
	}

	.swatch {
		flex: none;
		width: 1rem;
		height: 0.75rem;
		margin-top: 0.125rem;
	}

	.detail dl div {
		display: flex;
		flex-wrap: wrap;
		gap: 0 var(--space-2);
	}

	.detail dt {
		font-weight: 600;
	}

	.detail dd {
		font-variant-numeric: tabular-nums;
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

	.notes-title {
		font-weight: 600;
	}

	figcaption {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		font-size: var(--caption);
		color: var(--text-secondary);
	}
</style>
