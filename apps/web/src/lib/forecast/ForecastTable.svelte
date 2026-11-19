<!--
	The hours of one day as a table: a row for each height the wind is given
	at, the highest at the top, then the sky. The names of the rows stay in
	place and the hours scroll under a finger or a wheel. It is a table to a
	screen reader too, with the hour over each column and the row's name
	before each value.

	No colour says good or bad: the arrows are in the wind scale's blue, which
	is a magnitude, and the cloud squares are grey.
-->
<script lang="ts">
	import type { ForecastDay, ModelForecast } from '#lib/core/index.ts';
	import { compassPoint, format } from '#lib/design/format.ts';
	import WindGlyph from '#lib/wind/WindGlyph.svelte';
	import { hoursOf, rows, windInWords } from './table';

	interface Props {
		model: ModelForecast;
		day: ForecastDay;
	}

	let { model, day }: Props = $props();

	const text = format();
	const hours = $derived(hoursOf(day));
	const table = $derived(rows(model, hours, text));
</script>

<!-- A region the keyboard can reach, so that its hours can be scrolled without a mouse. -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div class="scroller" role="region" aria-label="The forecast, hour by hour" tabindex="0">
	<table class="numeric">
		<thead>
			<tr>
				<th class="name" scope="col"><span class="visually-hidden">Hour of the day</span></th>
				{#each hours as hour (hour.timeUnixS)}
					<th class="hour" scope="col">{text.hourOfDay(hour.timeUnixS)}</th>
				{/each}
			</tr>
		</thead>
		<tbody>
			{#each table as row (row.id)}
				<tr>
					<th class="name" scope="row">
						{row.name}
						{#if row.detail}
							<span class="detail">{row.detail}</span>
						{/if}
					</th>
					{#each row.cells as cell, index (hours[index].timeUnixS)}
						<td>
							{#if cell.is === 'wind'}
								<!-- An arrow pointing where the air moves, north at the top. -->
								<span class="wind">
									<WindGlyph
										mark={{
											symbol:
												cell.wind.fromDeg === undefined
													? { kind: 'calm' }
													: { kind: 'arrow', band: cell.wind.band },
											headingDeg: (cell.wind.fromDeg ?? 0) + 180,
											label: cell.wind.label
										}}
									/>
									<span aria-hidden="true">{cell.wind.label}</span>
									<span class="visually-hidden">{windInWords(cell.wind, compassPoint)}</span>
								</span>
							{:else if cell.is === 'cloud'}
								<!-- More sky covered, more of the square filled in. -->
								<span
									class="cloud"
									class:covered={cell.percent > 55}
									style:--cover={Math.min(Math.max(cell.percent, 0), 100) / 100}
								>
									{Math.round(cell.percent)}
								</span>
							{:else if cell.is === 'number'}
								{cell.text}
							{:else}
								<span class="nothing" aria-hidden="true">–</span>
							{/if}
						</td>
					{/each}
				</tr>
			{/each}
		</tbody>
	</table>
</div>

<style>
	.scroller {
		overflow-x: auto;
		/* The hours run to the card's edge; the names keep the card's inset. */
		margin-right: -1rem;
		padding-right: 1rem;
	}

	.scroller:focus-visible {
		outline-offset: 0.25rem;
		border-radius: 0.25rem;
	}

	table {
		border-collapse: collapse;
		font-size: var(--caption);
		font-weight: 500;
	}

	th,
	td {
		padding: 0;
		text-align: center;
		white-space: nowrap;
	}

	/* As wide as the iPhone's columns: a wind with its gusts ("15/61") and a
	   height ("1,600") in one hour must not run into the next. */
	td,
	.hour {
		min-width: 2.875rem;
		height: 2.5rem;
	}

	.hour {
		height: 1.375rem;
		color: var(--text-secondary);
	}

	/* The names of the rows stay where they are while the hours scroll. */
	.name {
		position: sticky;
		left: 0;
		z-index: 1;
		min-width: 5.5rem;
		padding-right: 0.5rem;
		background: var(--surface);
		font-family: var(--font);
		font-size: var(--footnote);
		text-align: left;
	}

	.detail {
		display: block;
		font-size: var(--caption);
		font-weight: 400;
		color: var(--text-secondary);
	}

	.wind {
		display: flex;
		flex-direction: column;
		align-items: center;
		line-height: 1;
	}

	/* The map's arrow, at the size of a cell. */
	.wind :global(svg) {
		width: 1.375rem;
		height: 1.375rem;
	}

	.cloud {
		display: inline-grid;
		place-items: center;
		width: 1.875rem;
		height: 1.375rem;
		border-radius: 0.25rem;
		background: color-mix(in srgb, var(--text) calc(8% + 50% * var(--cover)), transparent);
	}

	/* On a square that is mostly filled the number is set in the surface's colour. */
	.cloud.covered {
		color: var(--surface);
	}

	.nothing {
		color: var(--icon-quiet);
	}
</style>
