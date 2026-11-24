<!--
	The class of the glider one of the pilot's flights was flown on: for a
	flight brought in before the pilot set a class on the account, or with
	the wrong one. The classes are the core's names, "None" first; choosing
	one sends it at once and closes the sheet. A refusal stays in the sheet,
	in the core's words.
-->
<script lang="ts">
	import { getApp } from '#lib/app/app.svelte.ts';
	import type { GliderClassKind } from '#lib/core/index.ts';
	import Icon from '#lib/design/Icon.svelte';
	import Sheet from '#lib/design/Sheet.svelte';

	interface Props {
		open: boolean;
		/** The class the flight has now; none where it is not known. */
		chosen: GliderClassKind | undefined;
		/** Sends the class; resolves with why not, in words, or nothing when it was set. */
		onchoose: (gliderClass: GliderClassKind | undefined) => Promise<string | undefined>;
		onclose: () => void;
	}

	let { open, chosen, onchoose, onclose }: Props = $props();

	const app = getApp();
	const id = $props.id();
	const classes = $derived(app.said?.gliderClasses ?? []);
	const title = $derived(app.said?.glider.classTitle ?? '');

	let working = $state(false);
	/** Why the class was not set, in words. */
	let refusal = $state.raw<string>();

	$effect(() => {
		if (open) refusal = undefined;
	});

	async function choose(kind: GliderClassKind | undefined) {
		if (working) return;
		working = true;
		try {
			refusal = await onchoose(kind);
		} finally {
			working = false;
		}
		if (!refusal) onclose();
	}
</script>

<Sheet {open} {title} half {onclose}>
	<div class="group first">
		<div class="group-card" role="radiogroup" aria-label={title}>
			{#each classes as one (one.kind ?? '')}
				{@const picked = (chosen ?? '') === (one.kind ?? '')}
				<label class="choice" class:working>
					<input
						type="radio"
						name="{id}-class"
						value={one.kind ?? ''}
						checked={picked}
						disabled={working}
						onchange={() => choose(one.kind)}
					/>
					<span class="name">{one.name}</span>
					<span class="tick" class:picked><Icon name="check" /></span>
				</label>
			{/each}
		</div>
		{#if refusal}
			<p class="group-note refusal" role="alert">{refusal}</p>
		{/if}
	</div>
</Sheet>

<style>
	.first {
		margin-top: 0.25rem;
	}

	.choice {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		min-height: var(--target);
		padding: 0.625rem var(--space-4);
		cursor: pointer;
	}

	.choice.working {
		cursor: default;
		opacity: 0.6;
	}

	/* The radio button is not drawn, but it takes the keyboard: the tick at
	   the end of the row says which one is chosen. */
	.choice input {
		position: absolute;
		opacity: 0;
	}

	.choice:has(:focus-visible) {
		outline: 0.1875rem solid var(--color-focus);
		outline-offset: -0.1875rem;
		border-radius: 0.75rem;
	}

	.name {
		flex: 1;
		min-width: 0;
	}

	.tick {
		color: var(--link);
		visibility: hidden;
	}

	.tick.picked {
		visibility: visible;
	}

	.refusal {
		color: var(--danger);
	}

	@media (hover: hover) {
		.choice:not(.working):hover {
			background: var(--fill);
		}
	}
</style>
