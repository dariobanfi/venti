<!--
	One of a few: a row of choices with the chosen one raised, the iPhone's
	segmented control. The choices are radio buttons, so the arrow keys move
	between them and a screen reader says how many there are.
-->
<script lang="ts">
	interface Props {
		/** What is being chosen, for a screen reader. */
		label: string;
		/** The choices, by the words on them. */
		options: readonly string[];
		/** The chosen one. */
		value: string;
	}

	let { label, options, value = $bindable() }: Props = $props();

	/** The buttons of one control belong together and to no other on the page. */
	const name = $props.id();
</script>

<div class="segments" role="radiogroup" aria-label={label}>
	{#each options as option (option)}
		<label class:chosen={value === option}>
			<input type="radio" {name} value={option} bind:group={value} />
			{option}
		</label>
	{/each}
</div>

<style>
	.segments {
		display: flex;
		gap: 0.125rem;
		padding: 0.1875rem;
		border-radius: var(--radius-pill);
		background: var(--fill);
	}

	label {
		flex: 1;
		display: grid;
		place-items: center;
		min-height: 2.375rem;
		border-radius: var(--radius-pill);
		font-size: var(--subheadline);
		font-weight: 500;
		color: var(--text-secondary);
		cursor: pointer;
	}

	/* The pack's segmented control: the chosen one in the primary colour. */
	.chosen {
		background: var(--accent);
		color: var(--on-accent);
	}

	/* The radio buttons themselves are not drawn, but they take the keyboard. */
	input {
		position: absolute;
		opacity: 0;
	}

	label:has(:focus-visible) {
		outline: 0.1875rem solid var(--color-focus);
		outline-offset: 0.125rem;
	}
</style>
