<!--
	Other pilots' flights as a list: everyone's, or one pilot's. More is
	fetched with a button at its foot, and under it stands whose flights
	these are and whose reckoning the distances and points.
-->
<script lang="ts">
	import { MediaQuery } from 'svelte/reactivity';
	import Icon from '#lib/design/Icon.svelte';
	import type { ListState } from './pages';
	import PublicFlightRow from './PublicFlightRow.svelte';
	import PublicFlightTable, { type Said } from './PublicFlightTable.svelte';

	interface Props {
		/** What the list is called, for a screen reader. */
		label: string;
		list: ListState;
		/** Whether each row names its pilot. */
		named: boolean;
		/** Why a list without flights has none, and what would change that. */
		nothing: { title: string; says: string };
		/** Whose flights these are and whose reckoning the numbers. */
		reckoning: string;
		onmore: () => void;
		/** Asks again for a list that could not be fetched. */
		onagain: () => void;
		/** In a feed: the core's lines of each flight's cheers and comments. */
		said?: Said;
	}

	let { label, list, named, nothing, reckoning, onmore, onagain, said }: Props = $props();

	/** The two lines under a row of a phone's list. */
	const under = (id: string) => {
		const lines = said?.of[id];
		return lines ? [lines.cheers, lines.comments].filter(Boolean).join(' · ') : '';
	};

	/** A wide screen has a table whose headings sort; a phone, the list of rows. */
	const wide = new MediaQuery('min-width: 56rem', false);
</script>

{#if list.flights.length > 0}
	<div class="card" class:wide={wide.current}>
		{#if wide.current}
			<PublicFlightTable {label} flights={list.flights} {named} {said} />
		{:else}
			<ul aria-label={label}>
				{#each list.flights as flight (flight.id)}
					<PublicFlightRow {flight} {named} under={under(flight.id)} />
				{/each}
			</ul>
		{/if}
	</div>
	{#if list.problem}
		<p class="problem" role="status">{list.problem}</p>
	{/if}
	{#if list.more}
		<div class="more">
			<button class="button-plain" type="button" disabled={list.loading} onclick={onmore}>
				{list.loading ? 'Fetching…' : 'More flights'}
			</button>
		</div>
	{/if}
	<p class="footnote">{reckoning}</p>
{:else if list.loading || (!list.known && !list.problem && !list.absent)}
	<p class="waiting" role="status">Fetching the flights…</p>
{:else}
	<div class="nothing">
		<span class="symbol"><Icon name={list.problem ? 'warning' : 'person'} /></span>
		<h2>{nothing.title}</h2>
		<p>{nothing.says}</p>
		{#if list.problem}
			<button class="button-plain" type="button" onclick={onagain}>Try again</button>
		{/if}
	</div>
{/if}

<style>
	.card {
		background: var(--surface);
		border-radius: var(--radius-card);
		overflow: hidden;
	}

	.more {
		display: flex;
		justify-content: center;
		margin-top: 0.75rem;
	}

	.problem {
		margin-top: 0.75rem;
		padding: 0 1rem;
		font-size: var(--footnote);
		color: var(--danger);
	}

	.footnote {
		margin-top: 0.75rem;
		padding: 0 1rem;
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	.waiting {
		display: grid;
		place-items: center;
		min-height: 12rem;
		color: var(--text-secondary);
	}

	/* A list with nothing in it says so, and what to do next. */
	.nothing {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.375rem;
		max-width: 24rem;
		margin: 0 auto;
		padding: 4rem 0 3rem;
		text-align: center;
	}

	.symbol {
		color: var(--text-secondary);
		font-size: 2.25rem;
		margin-bottom: 0.5rem;
	}

	.nothing h2 {
		font-size: var(--title);
		font-weight: 600;
	}

	.nothing p {
		color: var(--text-secondary);
	}

	.nothing button {
		margin-top: 0.75rem;
	}

	/* A table on a wide screen: flat, with the pack's border. */
	.card.wide {
		border: 1px solid var(--color-border);
		border-radius: var(--radius-m);
		/* Clipped, not scrolled: the table's heading then sticks to the page. */
		overflow: clip;
	}
</style>
