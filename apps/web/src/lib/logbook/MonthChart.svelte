<!--
	A year of flying, one bar to the month. To a screen reader it is a table of
	the same months and values.
-->
<script lang="ts">
	import type { MonthStatistics } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import { roundTicks, scale } from '#lib/flight/chart.ts';

	interface Props {
		months: MonthStatistics[];
		/** The figure a bar stands for, and how it is written. */
		value: (month: MonthStatistics) => number;
		write: (value: number) => string;
		/** What the chart shows, in words: "Airtime by month, in hours". */
		label: string;
	}

	let { months, value, write, label }: Props = $props();

	const text = format();

	/** The size of the plot in pixels, as the browser laid it out. */
	let width = $state(0);
	let height = $state(0);

	const values = $derived(months.map(value));
	// The scale ends on a round value above the tallest bar, and never at nothing.
	const ticks = $derived(roundTicks(0, Math.max(...values, 0) * 1.3 || 1));
	const top = $derived(Math.max(...values, ticks.at(-1) ?? 0, Number.EPSILON));
	const y = $derived(scale([0, top], [height, 0]));

	/** Each month has a slot; its bar fills most of it. */
	const slot = $derived(width / Math.max(months.length, 1));
	const bar = $derived(Math.max(slot * 0.62, 1));
</script>

<div class="chart">
	<div class="plot" bind:clientWidth={width} bind:clientHeight={height}>
		<svg {width} {height} aria-hidden="true" focusable="false">
			{#each ticks as tick (tick)}
				<line class="grid" x1="0" x2={width} y1={y(tick)} y2={y(tick)} />
			{/each}
			<line class="grid" x1="0" x2={width} y1={height} y2={height} />
			{#each values as amount, index (index)}
				{#if amount > 0}
					<rect
						x={index * slot + (slot - bar) / 2}
						y={y(amount)}
						width={bar}
						height={Math.max(height - y(amount), 1)}
						rx="3"
					/>
				{/if}
			{/each}
		</svg>
	</div>
	<div class="amounts numeric" aria-hidden="true">
		<span class="widest">{write(top)}</span>
		{#each ticks as tick (tick)}
			<span style:top="{y(tick)}px">{write(tick)}</span>
		{/each}
	</div>
	<div class="months" aria-hidden="true">
		{#each months as month (`${month.year}-${month.month}`)}
			<span>{text.monthLetter(month.year, month.month)}</span>
		{/each}
	</div>
</div>

<table class="visually-hidden">
	<caption>{label}</caption>
	<tbody>
		{#each months as month, index (`${month.year}-${month.month}`)}
			<tr>
				<th scope="row">{text.monthName(month.year, month.month)}</th>
				<td>{write(values[index])}</td>
			</tr>
		{/each}
	</tbody>
</table>

<style>
	.chart {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		grid-template-rows: minmax(0, 1fr) auto;
		column-gap: 0.5rem;
		row-gap: 0.25rem;
		height: 11.5rem;
		font-size: var(--caption);
		color: var(--text-secondary);
	}

	.plot {
		position: relative;
		min-height: 0;
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

	rect {
		fill: var(--accent);
	}

	.amounts {
		position: relative;
	}

	/* Holds the column as wide as its widest label; the labels themselves
	   are placed over it. */
	.widest {
		visibility: hidden;
	}

	.amounts span:not(.widest) {
		position: absolute;
		left: 0;
		transform: translateY(-50%);
		white-space: nowrap;
	}

	.months {
		display: flex;
	}

	.months span {
		flex: 1;
		text-align: center;
	}
</style>
