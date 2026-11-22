<!--
	A pair's last 24 hours: the difference in pressure as a line, zero in
	the middle, the core's limits as thin lines across it with their names,
	and at the two sides what lies above and below, in the core's words.

	The line is the text's colour; each reading is a dot in the colour of
	the level the core gives it, and the limits' lines, with their names
	beside them, are the legend of those colours. A dot says its time and
	its difference to a mouse that rests on it.
-->
<script lang="ts">
	import type { FoehnLimit, FoehnPoint } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import { foehnChart } from './chart.ts';

	interface Props {
		points: FoehnPoint[];
		limits: FoehnLimit[];
		/** The heading over the chart: "Last 24 hours". */
		title: string;
		/** The chart's unit: "hPa". */
		unit: string;
		/** What lies above zero and below it, in the core's words. */
		above: string;
		below: string;
	}

	let { points, limits, title, unit, above, below }: Props = $props();

	const text = format();
	const shape = $derived(foehnChart(points, limits));
</script>

<figure class="chart" data-testid="foehn-chart">
	<figcaption>
		<span>{title}</span>
		<span class="unit">{unit}</span>
	</figcaption>
	<p class="side">{above}</p>
	<div class="plot" role="img" aria-label="{title}. {above}. {below}.">
		<div class="area">
			<svg viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden="true" focusable="false">
				<line class="zero" x1="0" x2="100" y1={shape.zeroY} y2={shape.zeroY} />
				{#each shape.lines as line (line.limit.differenceHpa)}
					<line class="limit {line.limit.level}" x1="0" x2="100" y1={line.y} y2={line.y} />
				{/each}
				{#if shape.path}
					<path class="line" d={shape.path} />
				{/if}
			</svg>
			{#each shape.points as placed (placed.point.timeUnixS)}
				<span
					class="dot {placed.point.level}"
					style:left="{placed.x}%"
					style:top="{placed.y}%"
					title="{text.time(placed.point.timeUnixS)}: {text.climb(
						placed.point.differenceHpa
					)} {unit}"
				></span>
			{/each}
		</div>
		<!-- Each limit's difference and its name, beside its line: the key to the lines and the dots. -->
		{#each shape.lines as line (line.limit.differenceHpa)}
			<span class="name numeric" style:top="{line.y}%" aria-hidden="true">
				{text.climb(line.limit.differenceHpa)}
				{line.limit.label}
			</span>
		{/each}
	</div>
	<p class="side">{below}</p>
</figure>

<style>
	.chart {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		margin: 0;
	}

	figcaption {
		display: flex;
		justify-content: space-between;
		gap: var(--space-3);
		font-size: var(--subheadline);
		font-weight: 500;
	}

	.unit,
	.side {
		color: var(--text-secondary);
	}

	.side {
		font-size: var(--caption);
	}

	.plot {
		position: relative;
		height: 7rem;
	}

	/* The readings, with room at the right for the limits' names. */
	.area {
		position: absolute;
		inset: 0 6.5rem 0 0;
	}

	svg {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		overflow: visible;
	}

	svg line,
	svg path {
		fill: none;
		vector-effect: non-scaling-stroke;
	}

	.zero {
		stroke: var(--color-border-strong);
		stroke-width: 1px;
	}

	.limit {
		stroke-width: 1px;
		stroke-dasharray: 4 3;
	}

	.limit.possible {
		stroke: var(--color-caution);
	}

	.limit.likely {
		stroke: var(--color-danger);
	}

	.limit.none,
	.limit.unknown {
		stroke: var(--color-border-strong);
	}

	.line {
		stroke: var(--text);
		stroke-width: 2px;
		stroke-linejoin: round;
		stroke-linecap: round;
	}

	/* A limit's difference and its name, at the height of its line. */
	.name {
		position: absolute;
		right: 0;
		width: 6.5rem;
		transform: translateY(-50%);
		padding-left: var(--space-2);
		font-size: var(--caption);
		line-height: 1.2;
		color: var(--text-secondary);
		white-space: nowrap;
	}

	.dot {
		position: absolute;
		width: var(--space-2);
		height: var(--space-2);
		margin: calc(var(--space-2) / -2) 0 0 calc(var(--space-2) / -2);
		border-radius: var(--radius-pill);
		background: var(--text-secondary);
		box-shadow: 0 0 0 1px var(--surface);
	}

	.dot.possible {
		background: var(--color-caution);
	}

	.dot.likely {
		background: var(--color-danger);
	}
</style>
