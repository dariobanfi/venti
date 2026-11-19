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
		padding: 0.125rem;
		border-radius: 0.625rem;
		background: var(--fill);
	}

	label {
		flex: 1;
		display: grid;
		place-items: center;
		min-height: 2rem;
		border-radius: 0.5rem;
		font-size: var(--subheadline);
		cursor: pointer;
	}

	.chosen {
		background: var(--surface);
		font-weight: 600;
		box-shadow: 0 1px 3px rgb(0 0 0 / 0.14);
	}

	@media (prefers-color-scheme: dark) {
		.chosen {
			background: var(--fill-strong);
		}
	}

	/* The radio buttons themselves are not drawn, but they take the keyboard. */
	input {
		position: absolute;
		opacity: 0;
	}

	label:has(:focus-visible) {
		outline: 0.1875rem solid var(--accent);
		outline-offset: 0.125rem;
	}
</style>
