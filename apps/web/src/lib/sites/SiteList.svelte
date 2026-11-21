<!--
	The flying sites around the part of the map in view, as a list: for
	looking one up by name, and for pilots who use the keyboard or a screen
	reader, which cannot reach signs drawn on a map. The words are those of
	the iPhone app's `SiteScreens.swift`.

	The map's filter is the list's: over the list stands what the filter is,
	as a row that unfolds its choices, and what it leaves; the sites that fit
	come first, and the take-offs whose source does not say what was asked
	come after them under the core's heading and sentence, apart and not
	left out.
-->
<script lang="ts">
	import type { SiteView } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import Sheet from '#lib/design/Sheet.svelte';
	import { listed } from './mark';
	import type { SitesModel } from './model.svelte';
	import SiteGlyph from './SiteGlyph.svelte';
	import SitesFilterCard from './SitesFilterCard.svelte';
	import type { SitesState } from './watch';
	import { asOf, CAUTION, detail, fromPilots, nothingListed } from './words';

	interface Props {
		open: boolean;
		sites: SitesState;
		/** The sites with the pilot's filter, once the core has started. */
		model: SitesModel | undefined;
		/** Called with the site the pilot chose. */
		onchoose: (site: SiteView) => void;
		onclose: () => void;
	}

	let { open, sites, model, onchoose, onclose }: Props = $props();

	const text = format();

	let search = $state('');

	/** Whether the filter's choices are unfolded over the list. */
	let choosing = $state(false);

	// A list opened again starts with every site the filter leaves.
	$effect(() => {
		if (!open) {
			search = '';
			choosing = false;
		}
	});

	/** What the filter leaves, in the core's words, once it has said so. */
	const left = $derived(model?.left);
	const shown = $derived(listed(model ? model.shown.fitting : sites.sites, search));
	/** The take-offs whose source does not say what the filter asks. */
	const apart = $derived(listed(model?.shown.unsaid ?? [], search));
</script>

{#snippet row(site: SiteView, isApart: boolean)}
	<li>
		<button type="button" onclick={() => onchoose(site)}>
			<SiteGlyph kind={site.kind} apart={isApart} />
			<span class="pair">
				<span class="name">{site.name}</span>
				<span class="lesser">{detail(site, text)}</span>
				{#if fromPilots(site)}
					<!-- A place from pilots says its tier under its name, in the core's label. -->
					<span class="lesser tier">{fromPilots(site)}</span>
				{/if}
			</span>
		</button>
	</li>
{/snippet}

<Sheet {open} title="Flying sites" {onclose}>
	{#if sites.sites.length === 0}
		<div class="nothing">
			<h3>No flying sites here</h3>
			<p>{nothingListed(sites)}</p>
		</div>
	{:else}
		<input
			type="search"
			placeholder="Site"
			aria-label="Site"
			autocomplete="off"
			bind:value={search}
		/>
		{#if model && left}
			<!-- What the filter is, where a list's title stands: the row unfolds its choices. -->
			<button
				class="fold"
				type="button"
				aria-expanded={choosing}
				onclick={() => (choosing = !choosing)}
			>
				<Icon name="filter" />
				<span class="filter-words">{left.filterWords}</span>
				<span class="turn" class:up={choosing} aria-hidden="true"><Icon name="forward" /></span>
			</button>
			{#if choosing}
				<div class="choices">
					<SitesFilterCard sites={model} compact />
				</div>
			{:else}
				<p class="left numeric" role="status">{left.words}</p>
				{#if left.fromPilotsWords}
					<p class="left numeric">{left.fromPilotsWords}</p>
				{/if}
			{/if}
		{/if}
		{#if shown.length === 0 && apart.length === 0 && search.trim() !== ''}
			<div class="nothing">
				<h3>No results for “{search.trim()}”</h3>
				<p>Check the spelling or try a new search.</p>
			</div>
		{:else}
			{#if shown.length > 0}
				<ul class="card" aria-label="Flying sites">
					{#each shown as site (site.id)}
						{@render row(site, false)}
					{/each}
				</ul>
			{/if}
			{#if apart.length > 0 && left}
				<!-- Neither a fit nor left out: apart, under the core's heading and sentence. -->
				<h3 class="apart numeric">{left.unsaidTitle}</h3>
				<p class="apart-note">{left.unsaidNote}</p>
				<ul class="card" aria-label={left.unsaidTitle}>
					{#each apart as site (site.id)}
						{@render row(site, true)}
					{/each}
				</ul>
			{/if}
		{/if}
		<!-- What a sign on a map is worth is said wherever sites are listed. -->
		<p class="footnote">
			<span>{CAUTION}</span>
			{#each sites.credits as credit (credit.source)}
				<span>{credit.text}</span>
			{/each}
			<span>{asOf(sites.asOfUnixS, text)}</span>
		</p>
	{/if}
</Sheet>

<style>
	input {
		width: 100%;
		min-height: 2.5rem;
		margin-bottom: 0.75rem;
		padding: 0 0.875rem;
		border: 0;
		border-radius: 0.75rem;
		background: var(--fill);
		color: var(--text);
		font: inherit;
	}

	input::placeholder {
		color: var(--text-secondary);
	}

	.card {
		background: var(--surface);
		border-radius: var(--radius-card);
		overflow: hidden;
	}

	li + li {
		box-shadow: inset 0 0.5px 0 var(--separator);
	}

	/* The filter in a line, a row to press: the pack's link colour, as the
	   forecast card's "Key and notes". */
	.fold {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		width: 100%;
		min-height: var(--target);
		color: var(--link);
		font-size: var(--subheadline);
		font-weight: 600;
		text-align: left;
	}

	.fold :global(svg) {
		width: 1.125rem;
		height: 1.125rem;
	}

	.filter-words {
		flex: 1;
		min-width: 0;
	}

	/* The pack's forward chevron, turned to point down, and up once unfolded. */
	.turn {
		display: grid;
		transform: rotate(90deg);
	}

	.turn.up {
		transform: rotate(-90deg);
	}

	.choices {
		margin-bottom: var(--space-3);
		padding: var(--space-3);
		border-radius: var(--radius-card);
		background: var(--surface);
	}

	.left,
	.apart-note {
		margin: 0 0 var(--space-2);
		padding: 0 var(--space-1);
		color: var(--text-secondary);
		font-size: var(--footnote);
	}

	.apart {
		margin: var(--space-4) 0 var(--space-1);
		padding: 0 var(--space-1);
		font-size: var(--subheadline);
		font-weight: 600;
	}

	li button {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		width: 100%;
		min-height: 3.25rem;
		padding: 0.375rem 1rem 0.375rem 0.75rem;
		text-align: left;
	}

	@media (hover: hover) {
		li button:hover {
			background: var(--fill);
		}
	}

	li button:active {
		background: var(--fill-strong);
	}

	li button:focus-visible {
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
		font-weight: 600;
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
		overflow-wrap: anywhere;
	}
</style>
