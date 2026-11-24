<!--
	What airspace lies over a place on the map, from the lowest floor up, as
	the core lists it: each with its floor and ceiling, how Venti treats it
	and why, and what its source publishes about its use. At the foot the
	filter by height for every map's airspace. Every sentence is the core's.
-->
<script lang="ts">
	import {
		type AirspaceHeightFilter,
		type AirspaceOverPlace,
		type AirspacePolicy,
		type Coordinate,
		inWordsOr
	} from '#lib/core/index.ts';
	import Segmented from '#lib/design/Segmented.svelte';
	import StatusChip from '#lib/today/StatusChip.svelte';
	import type { AirspaceModel } from './model.svelte';

	interface Props {
		place: Coordinate;
		airspace: AirspaceModel;
	}

	let { place, airspace }: Props = $props();

	type Phase =
		{ is: 'loading' } | { is: 'ready'; over: AirspaceOverPlace } | { is: 'failed'; why: string };

	let phase = $state.raw<Phase>({ is: 'loading' });
	let filter = $state.raw<AirspaceHeightFilter>();
	/** Counts the requests, so that a slow answer for another place is dropped. */
	let asked = 0;

	function load(at: Coordinate) {
		const mine = ++asked;
		phase = { is: 'loading' };
		airspace.over(at).then(
			(over) => {
				if (mine === asked) phase = { is: 'ready', over };
			},
			(error) => {
				if (mine !== asked) return;
				phase = {
					is: 'failed',
					why: inWordsOr('The airspace could not be read.', error)
				};
			}
		);
	}

	$effect(() => load(place));
	$effect(() => {
		void airspace.heightFilter().then((steps) => (filter = steps));
	});

	/** How Venti treats an airspace, in the pack's status roles, as the map colours it. */
	const VERDICT = {
		avoid: 'notFlyable',
		caution: 'marginal',
		free: 'flyable'
	} as const satisfies Record<AirspacePolicy, string>;

	/** The filter's choices by their words, and the one chosen. */
	const words = $derived(filter?.steps.map((step) => step.words) ?? []);
	const chosen = $derived(
		filter?.steps.find((step) => step.ceilingM === airspace.ceilingM)?.words ?? words[0] ?? ''
	);

	function choose(picked: string) {
		const step = filter?.steps.find((one) => one.words === picked);
		if (step) airspace.filterBelow(step.ceilingM);
	}
</script>

<div class="over">
	{#if phase.is === 'loading'}
		<p class="waiting" role="status">Loading…</p>
	{:else if phase.is === 'failed'}
		<p class="card">{phase.why}</p>
	{:else}
		{@const over = phase.over}
		<section>
			<p class="head">{over.words}</p>
			{#if over.coverNote}<p class="footnote">{over.coverNote}</p>{/if}
		</section>
		{#if over.airspaces.length > 0}
			<ul class="list" aria-label="Airspace over this place">
				{#each over.airspaces as each (each.id)}
					<li class="card">
						<div class="title">
							<h3>{each.name}</h3>
							<StatusChip verdict={VERDICT[each.policy]} status={each.policyWords} />
						</div>
						<p class="lesser">{each.kind}</p>
						<p class="numeric">{each.lower} to {each.upper}</p>
						<p class="lesser">{each.why}</p>
						{#if each.publishedUse}<p>{each.publishedUse}</p>{/if}
						{#if each.hours.length > 0}
							<ul class="hours" aria-label="Published hours">
								{#each each.hours as line (line)}<li>{line}</li>{/each}
							</ul>
						{/if}
						{#if each.hoursRemarks}<p class="lesser">{each.hoursRemarks}</p>{/if}
						{#if each.remarks}<p class="lesser">{each.remarks}</p>{/if}
					</li>
				{/each}
			</ul>
		{/if}
		{#if over.hoursNote}<p class="footnote">{over.hoursNote}</p>{/if}
		<p class="footnote">{over.caveat}</p>
	{/if}

	{#if filter}
		<section class="filter">
			<h3>{filter.title}</h3>
			<Segmented
				label={filter.title}
				options={words}
				bind:value={() => chosen, (picked) => choose(picked)}
			/>
			<p class="footnote">{filter.note}</p>
		</section>
	{/if}
</div>

<style>
	.over {
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}

	.head {
		padding: 0 1rem;
		font-weight: 600;
	}

	.list {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		list-style: none;
		padding: 0;
		margin: 0;
	}

	.card {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		background: var(--surface);
		border-radius: var(--radius-card);
		padding: 1rem;
		overflow-wrap: anywhere;
	}

	.title {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: 0.5rem;
	}

	h3 {
		font-size: var(--body);
		font-weight: 600;
	}

	.filter h3 {
		padding: 0 1rem 0.375rem;
		font-size: var(--subheadline);
		color: var(--text-secondary);
	}

	.hours {
		list-style: none;
		padding: 0;
		margin: 0;
		font-size: var(--subheadline);
	}

	.lesser {
		font-size: var(--subheadline);
		color: var(--text-secondary);
	}

	.footnote {
		padding: 0.5rem 1rem 0;
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	.waiting {
		display: grid;
		place-items: center;
		min-height: 7.5rem;
		color: var(--text-secondary);
	}
</style>
