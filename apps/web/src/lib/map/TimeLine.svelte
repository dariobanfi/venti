<!--
	The map's one line of time (`timeline.ts`), in the manner of the pack's
	hour strip (`.v-hours`): a cell a stop, the hour over a bar, the chosen
	cell's bar in the primary colour. What was measured is narrower and its
	bar darker; over the line stand the days' names and "Now", with a rule
	where it is. Longer than its card, it scrolls sideways and keeps the
	chosen cell in view.

	To the keyboard and to a screen reader it is one adjustable value that
	says the stop: the arrow keys step a stop, Home and End go to the first
	and the last. A click or a tap chooses the cell under it.
-->
<script lang="ts">
	import { WORDS, type Cell } from './timeline';

	interface Props {
		label: string;
		cells: readonly Cell[];
		/** The chosen cell, by its place. */
		value: number;
		onchange: (index: number) => void;
	}

	let { label, cells, value, onchange }: Props = $props();

	let strip = $state<HTMLDivElement>();

	/**
	 * How far each word over the line may reach: over its cell and those
	 * after it up to the next word, which it never runs into. The last
	 * reaches as far as it needs.
	 */
	const rooms = $derived.by(() => {
		const said = (cell: Cell) => Boolean(cell.now || cell.over);
		return cells.map((cell, index) => {
			if (!said(cell)) return undefined;
			const next = cells.findIndex((other, at) => at > index && said(other));
			if (next === -1) return undefined;
			const reach = cells.slice(index, next);
			const narrow = reach.filter((one) => one.narrow).length;
			const days = cells.slice(index + 1, next + 1).filter((one) => one.startsDay).length;
			return `calc(${reach.length - narrow} * var(--space-6) + ${narrow} * var(--space-3) + ${
				reach.length - 1
			} * var(--cell-gap) + ${days} * var(--space-2))`;
		});
	});

	function choose(index: number) {
		const to = Math.min(Math.max(index, 0), cells.length - 1);
		if (to !== value) onchange(to);
	}

	/** The cell a click landed on. */
	function clicked(event: MouseEvent) {
		const cell = (event.target as Element | null)?.closest<HTMLElement>('[data-index]');
		if (cell) choose(Number(cell.dataset.index));
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

	// The chosen cell stays in view, with a little of the line on each side.
	$effect(() => {
		const cell = strip?.children[value] as HTMLElement | undefined;
		if (!strip || !cell) return;
		const room = cell.offsetWidth * 2;
		const left = cell.offsetLeft - room;
		const right = cell.offsetLeft + cell.offsetWidth + room - strip.clientWidth;
		if (strip.scrollLeft > left) strip.scrollLeft = Math.max(left, 0);
		else if (strip.scrollLeft < right) strip.scrollLeft = right;
	});
</script>

<!-- svelte-ignore a11y_click_events_have_key_events: the slider takes the keys. -->
<div
	bind:this={strip}
	class="line"
	role="slider"
	tabindex="0"
	aria-label={label}
	aria-valuemin={0}
	aria-valuemax={Math.max(cells.length - 1, 0)}
	aria-valuenow={value}
	aria-valuetext={cells[value]?.label}
	onclick={clicked}
	onkeydown={keyed}
>
	{#each cells as cell, index (index)}
		<span
			class="cell numeric"
			class:narrow={cell.narrow}
			class:measured={cell.measured}
			class:chosen={index === value}
			class:now={cell.now}
			class:starts-day={cell.startsDay}
			data-index={index}
			aria-hidden="true"
		>
			<b style:max-width={rooms[index]}>{cell.now ? WORDS.now : (cell.over ?? '')}</b>
			<span class="short">{cell.short}</span>
			<i></i>
		</span>
	{/each}
</div>

<style>
	.line {
		--cell-gap: 2px;
		position: relative;
		display: flex;
		gap: var(--cell-gap);
		min-width: 0;
		overflow-x: auto;
		scrollbar-width: none;
		color: var(--text-secondary);
		font-size: var(--caption);
		cursor: pointer;
		user-select: none;
	}

	.line::-webkit-scrollbar {
		display: none;
	}

	.cell {
		position: relative;
		flex: none;
		display: grid;
		grid-template-rows: auto auto auto;
		grid-template-columns: minmax(0, 1fr);
		gap: var(--space-1);
		width: var(--space-6);
		padding-top: var(--space-1);
		text-align: center;
	}

	.cell.narrow {
		width: var(--space-3);
	}

	/* A day begins a little apart from the one before. */
	.cell.starts-day {
		margin-left: var(--space-2);
	}

	/* A day's name, a radar's full hour or "Now", over the cell it begins at;
	   it may reach over the next ones, up to the next word (`rooms`). */
	.cell b {
		width: max-content;
		min-height: 1lh;
		font-weight: 600;
		text-align: left;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		color: var(--text);
	}

	.short {
		min-height: 1lh;
		white-space: nowrap;
		overflow: visible;
		font-weight: 600;
	}

	.cell i {
		height: var(--space-2);
		background: var(--fill);
	}

	.measured i {
		background: var(--fill-strong);
	}

	.cell:first-child i {
		border-radius: var(--space-1) 0 0 var(--space-1);
	}

	.cell:last-child i {
		border-radius: 0 var(--space-1) var(--space-1) 0;
	}

	/* Now: a rule down the cell's left edge, from the word to the bar. */
	.now::before {
		content: '';
		position: absolute;
		top: 0;
		bottom: 0;
		left: -2px;
		width: 2px;
		background: var(--text);
	}

	.now:first-child::before {
		left: 0;
	}

	.now b {
		padding-left: var(--space-1);
	}

	.chosen {
		color: var(--text);
	}

	.chosen i {
		background: var(--accent);
	}

	.line:focus-visible {
		outline: 2px solid var(--color-focus);
		outline-offset: 2px;
	}
</style>
