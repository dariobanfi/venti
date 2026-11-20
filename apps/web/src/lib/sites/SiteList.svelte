<!--
	The flying sites around the part of the map in view, as a list: for
	looking one up by name, and for pilots who use the keyboard or a screen
	reader, which cannot reach signs drawn on a map. The words are those of
	the iPhone app's `SiteScreens.swift`.
-->
<script lang="ts">
	import type { SiteView } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import Sheet from '#lib/design/Sheet.svelte';
	import { listed } from './mark';
	import SiteGlyph from './SiteGlyph.svelte';
	import type { SitesState } from './watch';
	import { asOf, CAUTION, detail, nothingListed } from './words';

	interface Props {
		open: boolean;
		sites: SitesState;
		/** Called with the site the pilot chose. */
		onchoose: (site: SiteView) => void;
		onclose: () => void;
	}

	let { open, sites, onchoose, onclose }: Props = $props();

	const text = format();

	let search = $state('');

	// A list opened again starts with every site.
	$effect(() => {
		if (!open) search = '';
	});

	const shown = $derived(listed(sites.sites, search));
</script>

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
		{#if shown.length === 0}
			<div class="nothing">
				<h3>No results for “{search.trim()}”</h3>
				<p>Check the spelling or try a new search.</p>
			</div>
		{:else}
			<ul class="card" aria-label="Flying sites">
				{#each shown as site (site.id)}
					<li>
						<button type="button" onclick={() => onchoose(site)}>
							<SiteGlyph kind={site.kind} />
							<span class="pair">
								<span class="name">{site.name}</span>
								<span class="lesser">{detail(site, text)}</span>
							</span>
						</button>
					</li>
				{/each}
			</ul>
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
