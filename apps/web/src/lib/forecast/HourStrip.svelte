<!--
	The line of hours of the forecast map: the pack's hour strip (`.v-hours`),
	a cell for each of the day's hours with the clock's hour in it, the
	chosen cell marked. A click chooses an hour, a finger or the mouse drawn
	along the strip chooses hour by hour. To the keyboard and to a screen
	reader it is one adjustable value that says the hour: the arrow keys
	step an hour, Home and End go to the first and the last.
-->
<script lang="ts">
	interface Cell {
		/** What the cell shows: the clock's hour, "13", or "Day". */
		short: string;
		/** What a screen reader says: "13:00", or "Day". */
		label: string;
	}

	interface Props {
		label: string;
		cells: readonly Cell[];
		/** The chosen cell, by its place. */
		value: number;
		onchange: (index: number) => void;
	}

	let { label, cells, value, onchange }: Props = $props();

	let strip = $state<HTMLDivElement>();

	function choose(index: number) {
		const to = Math.min(Math.max(index, 0), cells.length - 1);
		if (to !== value) onchange(to);
	}

	/** The cell under a pointer. */
	function at(event: PointerEvent): number {
		const box = strip!.getBoundingClientRect();
		const along = (event.clientX - box.left) / Math.max(box.width, 1);
		return Math.floor(Math.min(Math.max(along, 0), 0.9999) * cells.length);
	}

	function pressed(event: PointerEvent) {
		strip!.setPointerCapture(event.pointerId);
		choose(at(event));
	}

	function moved(event: PointerEvent) {
		if (strip!.hasPointerCapture(event.pointerId)) choose(at(event));
	}

	const STEPS: Record<string, number> = {
		ArrowRight: 1,
		ArrowUp: 1,
		ArrowLeft: -1,
		ArrowDown: -1
	};

	function keyed(event: KeyboardEvent) {
		if (event.key in STEPS) choose(value + STEPS[event.key]);
		else if (event.key === 'Home') choose(0);
		else if (event.key === 'End') choose(cells.length - 1);
		else return;
		event.preventDefault();
	}
</script>

<div
	bind:this={strip}
	class="hours"
	role="slider"
	tabindex="0"
	aria-label={label}
	aria-valuemin={0}
	aria-valuemax={cells.length - 1}
	aria-valuenow={value}
	aria-valuetext={cells[value]?.label}
	onpointerdown={pressed}
	onpointermove={moved}
	onkeydown={keyed}
>
	{#each cells as cell, index (index)}
		<span class="cell numeric" class:chosen={index === value} aria-hidden="true">
			{cell.short}
			<i></i>
		</span>
	{/each}
</div>

<style>
	/* The pack's hour strip: equal columns, the hour over a bar; its 4 px
	   rounding is the pack's smallest step of space. */
	.hours {
		display: grid;
		grid-auto-flow: column;
		grid-auto-columns: 1fr;
		gap: 2px;
		min-height: var(--target);
		align-items: end;
		text-align: center;
		font-size: var(--subheadline);
		font-weight: 600;
		color: var(--text-secondary);
		cursor: pointer;
		touch-action: none;
		user-select: none;
	}

	.cell {
		display: grid;
		justify-items: center;
		gap: var(--space-1);
		min-width: 0;
	}

	.cell i {
		width: 100%;
		height: 0.625rem;
		background: var(--fill);
	}

	.cell:first-child i {
		border-radius: var(--space-1) 0 0 var(--space-1);
	}

	.cell:last-child i {
		border-radius: 0 var(--space-1) var(--space-1) 0;
	}

	.chosen {
		color: var(--text);
	}

	.chosen i {
		background: var(--accent);
	}
</style>
