<!--
	The gliders on the map as a list: for reading them side by side, and for
	pilots who use the keyboard or a screen reader, which cannot reach wings
	drawn on a map. The words are those of the iPhone app's
	`LiveScreens.swift`.
-->
<script lang="ts">
	import type { LiveGlider } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import Sheet from '#lib/design/Sheet.svelte';
	import GliderGlyph from './GliderGlyph.svelte';
	import { listed } from './mark';
	import type { LiveModel } from './model.svelte';
	import { CAUTION, detail, nobodyListed, TRUNCATED } from './words';

	interface Props {
		open: boolean;
		live: LiveModel;
		/** Called with the glider the pilot chose. */
		onchoose: (glider: LiveGlider) => void;
		onclose: () => void;
	}

	let { open, live, onchoose, onclose }: Props = $props();

	const text = format();
	const shown = $derived(listed(live.state.gliders));
	const nobody = $derived(nobodyListed(live.state));

	// How old each position is, second by second, while the list is read.
	$effect(() => {
		if (open) return live.tellAges();
	});
</script>

<Sheet {open} title="In the air" {onclose}>
	{#if shown.length === 0}
		<div class="nothing">
			<h3>{nobody.title}</h3>
			<p>{nobody.says}</p>
		</div>
	{:else}
		<ul class="card" aria-label="In the air">
			{#each shown as glider (glider.id)}
				<li>
					<button type="button" onclick={() => onchoose(glider)}>
						<GliderGlyph {glider} />
						<span class="pair">
							<span class="name">{glider.name}</span>
							<span class="lesser numeric">{detail(glider, live.ageInWords(glider), text)}</span>
						</span>
					</button>
				</li>
			{/each}
		</ul>
		<!-- What a glider on a map is worth is said wherever gliders are listed. -->
		<p class="footnote">
			{#if live.state.truncated}<span>{TRUNCATED}</span>{/if}
			<span>{CAUTION}</span>
		</p>
	{/if}
</Sheet>

<style>
	.card {
		background: var(--surface);
		border-radius: var(--radius-card);
		overflow: hidden;
	}

	li + li {
		box-shadow: inset 0 0.5px 0 var(--separator);
	}

	button {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		width: 100%;
		min-height: 3.25rem;
		padding: 0.375rem 1rem 0.375rem 0.75rem;
		text-align: left;
	}

	@media (hover: hover) {
		button:hover {
			background: var(--fill);
		}
	}

	button:active {
		background: var(--fill-strong);
	}

	button:focus-visible {
		outline-offset: -0.1875rem;
		border-radius: 0.5rem;
	}

	.pair {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
		min-width: 0;
	}

	.name {
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.lesser {
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	.nothing {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.375rem;
		padding: 3rem 1.5rem;
		text-align: center;
	}

	.nothing h3 {
		font-size: var(--title);
		font-weight: 700;
	}

	.nothing p {
		color: var(--text-secondary);
	}

	.footnote {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		padding: 0.5rem 1rem 0;
		font-size: var(--footnote);
		color: var(--text-secondary);
	}
</style>
