<!--
	A square's days side by side, as the core gives them (`thermalMapStrip`):
	each day's name, and under it the square's kind of day as a mark in the
	map's colour and its status in words. The day the map shows is marked;
	a click on another day shows that day on the map. A square's sheet has
	it at its head, for the day as a whole and for an hour alike.
-->
<script lang="ts">
	import type { Coordinate, ThermalStripDay } from '#lib/core/index.ts';
	import { NOT_FLYABLE_ON_MAP, THERMAL_ON_MAP } from '#lib/design/palette.ts';
	import ThermalMark from '#lib/today/ThermalMark.svelte';
	import type { ThermalMapModel } from './thermal-map.svelte';

	interface Props {
		thermals: ThermalMapModel;
		/** A place in the square. */
		place: Coordinate;
	}

	let { thermals, place }: Props = $props();

	let days = $state.raw<ThermalStripDay[]>([]);

	// The days of the place, again with every map.
	$effect(() => {
		void thermals.map;
		const at = place;
		void thermals.strip(at).then((read) => {
			if (at === place) days = read;
		});
	});

	/** The mark's colour: the square's step of the thermal scale, the veil on a day not flyable. */
	function color(day: ThermalStripDay): string | undefined {
		const square = day.square;
		if (!square) return undefined;
		if (square.scaleStep !== undefined) return THERMAL_ON_MAP[square.scaleStep - 1];
		return square.verdict === 'notFlyable' ? NOT_FLYABLE_ON_MAP : undefined;
	}
</script>

{#if days.length > 0}
	<div class="days" role="group" aria-label="This square, day by day">
		{#each days as day (day.date)}
			<button
				class="day"
				type="button"
				aria-pressed={day.date === thermals.day?.date}
				onclick={() => thermals.chooseDay(day.date)}
			>
				<span class="name">{day.name}</span>
				<span class="reading">
					{#if color(day)}<ThermalMark color={color(day)} />{/if}
					<span>{day.square?.thermalWords || day.square?.status || '–'}</span>
				</span>
			</button>
		{/each}
	</div>
{/if}

<style>
	.days {
		display: grid;
		grid-auto-flow: column;
		grid-auto-columns: minmax(0, 1fr);
		gap: var(--space-1);
	}

	.day {
		display: grid;
		gap: var(--space-1);
		justify-items: center;
		min-height: var(--target);
		padding: var(--space-2) var(--space-1);
		border-radius: var(--radius-control);
		background: var(--fill);
		color: var(--text);
		text-align: center;
	}

	.day[aria-pressed='true'] {
		box-shadow: inset 0 0 0 0.125rem var(--accent);
	}

	.name {
		font-size: var(--subheadline);
		font-weight: 600;
	}

	.reading {
		display: flex;
		flex-wrap: wrap;
		justify-content: center;
		align-items: center;
		gap: var(--space-1);
		font-size: var(--caption);
		color: var(--text-secondary);
		overflow-wrap: anywhere;
	}
</style>
