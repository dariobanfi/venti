<!--
	A choice of years as the pack's chips, the chosen one on with its tick.
	The two newest years stand; the older ones wait behind a "More" chip,
	which shows them, and "Less" puts them away again (the owner,
	2026-11-23). A choice that is not a year, as "All years", always stands,
	where the core put it. A year chosen among the older ones keeps them
	shown.

	The choices are radio buttons, as in `Segmented.svelte`: the arrow keys
	move between them and a screen reader says how many there are.
-->
<script lang="ts">
	import ChipTick from './ChipTick.svelte';

	interface Props {
		/** What is being chosen, for a screen reader. */
		label: string;
		/** The choices, by the words on them, as the core orders them. */
		options: readonly string[];
		/** The chosen one. */
		value: string;
		/** Told the choice. */
		onchoose: (option: string) => void;
	}

	let { label, options, value, onchoose }: Props = $props();

	/** How many of the newest years stand without "More". */
	const SHOWN = 2;
	const isYear = (option: string) => /^\d{4}$/.test(option);

	const years = $derived(options.filter(isYear));
	const older = $derived(years.slice(SHOWN));
	let unfolded = $state(false);
	/** The older years are shown when asked for, and while one of them is chosen. */
	const showingOlder = $derived(unfolded || older.includes(value));

	/**
	 * The chips in the core's order, the "More" or "Less" chip after the
	 * years; none while an older year is chosen, which keeps them shown.
	 */
	const shown = $derived.by(() => {
		const lastYear = years.at(-1);
		const folds = older.length > 0 && !older.includes(value);
		const out: ({ option: string } | { more: true })[] = [];
		for (const option of options) {
			if (older.includes(option) && !showingOlder) {
				if (option === lastYear) out.push({ more: true });
				continue;
			}
			out.push({ option });
			if (option === lastYear && folds) out.push({ more: true });
		}
		return out;
	});

	const name = $props.id();
</script>

<div class="chips" role="radiogroup" aria-label={label}>
	{#each shown as chip ('more' in chip ? '\u0000more' : chip.option)}
		{#if 'more' in chip}
			<button
				class="chip"
				type="button"
				aria-expanded={showingOlder}
				onclick={() => (unfolded = !unfolded)}
			>
				{showingOlder ? 'Less' : 'More'}
			</button>
		{:else}
			<label class="chip" class:chosen={value === chip.option}>
				<input
					type="radio"
					{name}
					value={chip.option}
					checked={value === chip.option}
					onchange={() => onchoose(chip.option)}
				/>
				<ChipTick on={value === chip.option} />
				{chip.option}
			</label>
		{/if}
	{/each}
</div>

<style>
	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
	}

	/* The pack's chip. */
	.chip {
		display: inline-flex;
		align-items: center;
		min-height: calc(var(--target) - 0.625rem);
		padding: 0 var(--space-3);
		border-radius: var(--radius-pill);
		background: var(--fill);
		color: var(--text-secondary);
		font-size: var(--subheadline);
		font-weight: 500;
		cursor: pointer;
	}

	.chip.chosen {
		background: var(--accent);
		color: var(--on-accent);
	}

	button.chip {
		color: var(--link);
	}

	/* The radio buttons themselves are not drawn, but they take the keyboard. */
	input {
		position: absolute;
		opacity: 0;
	}

	.chip:has(:focus-visible) {
		outline: 0.1875rem solid var(--color-focus);
		outline-offset: 0.125rem;
	}
</style>
