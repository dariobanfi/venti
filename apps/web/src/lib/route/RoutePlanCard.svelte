<!--
	What a route drawn on the map comes to, in the core's words: its
	sentence, what the route would score flown as drawn (the core's name for
	it, "Closed FAI triangle", its distance and points), and each leg's
	length and direction. Under them "Undo", "Back to the start" and
	"Clear". A card at the map's side on a wide screen, a card over the
	map's foot on a narrow one, so that the map stays in reach of the finger.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import Metric from '#lib/design/Metric.svelte';
	import type { RoutePlanner } from './planner.svelte';

	interface Props {
		planner: RoutePlanner;
		onclose: () => void;
		/**
		 * The weather along the route: a place kept for it under the score,
		 * for when the core says what the forecast is along a route. Nothing
		 * is shown there until then.
		 */
		weather?: Snippet;
	}

	let { planner, onclose, weather }: Props = $props();

	const text = format();
	const plan = $derived(planner.plan);
	const score = $derived(plan?.score);

	/** What a place is called in the list of legs: the site's name, or its number. */
	function placeName(index: number): string {
		return planner.places[index]?.name ?? String(index + 1);
	}
</script>

<section class="route" aria-labelledby="route-title">
	<header>
		<h2 id="route-title">Plan a route</h2>
		<button type="button" aria-label="Close" title="Close" onclick={onclose}>
			<Icon name="close" />
		</button>
	</header>

	{#if planner.problem}
		<p class="problem" role="alert">{planner.problem}</p>
	{:else if plan}
		<p class="words" role="status" data-route-words>{plan.words}</p>
	{/if}

	{#if score}
		<div class="score">
			<h3>{score.name}</h3>
			<dl class="metrics">
				<Metric caption="Scoring distance" value={text.kilometres(score.distanceM)} unit="km" />
				<Metric caption="Points" value={text.points(score.points)} />
				<Metric caption="Along the route" value={text.kilometres(plan?.distanceM ?? 0)} unit="km" />
			</dl>
		</div>
	{/if}

	{#if weather}
		<!-- The weather along the route: the core's, once it has it. -->
		<section class="weather" aria-label="The weather along the route">
			{@render weather()}
		</section>
	{/if}

	{#if plan && plan.legs.length > 0}
		<ol class="legs" aria-label="Legs">
			{#each plan.legs as leg, index (index)}
				<li>
					<span class="from-to">{placeName(index)} → {placeName(index + 1)}</span>
					<span class="numeric">{leg.words}</span>
				</li>
			{/each}
		</ol>
	{/if}

	<div class="acts">
		<button
			class="button-plain"
			type="button"
			disabled={planner.places.length === 0}
			onclick={() => planner.undo()}
		>
			Undo
		</button>
		<button
			class="button-plain"
			type="button"
			disabled={!planner.canClose}
			onclick={() => planner.backToStart()}
		>
			Back to the start
		</button>
		<button
			class="button-plain clear"
			type="button"
			disabled={planner.places.length === 0}
			onclick={() => planner.clear()}
		>
			<Icon name="trash" />
			Clear
		</button>
	</div>
</section>

<style>
	.route {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
	}

	h2 {
		margin: 0;
		font-size: var(--subheadline);
		font-weight: 600;
	}

	header button {
		display: grid;
		place-items: center;
		width: var(--target);
		height: var(--target);
		margin: calc(var(--space-2) * -1);
		color: var(--text-secondary);
	}

	.words {
		font-size: var(--subheadline);
	}

	.problem {
		font-size: var(--subheadline);
		color: var(--danger);
	}

	.score {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		padding-top: var(--space-2);
		border-top: 1px solid var(--separator);
	}

	h3 {
		margin: 0;
		font-size: var(--subheadline);
		font-weight: 600;
	}

	.metrics {
		display: flex;
		gap: var(--space-3);
	}

	.legs {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		margin: 0;
		padding: var(--space-2) 0 0;
		border-top: 1px solid var(--separator);
		list-style: none;
		font-size: var(--footnote);
	}

	.legs li {
		display: flex;
		justify-content: space-between;
		gap: var(--space-3);
	}

	.from-to {
		min-width: 0;
		overflow-wrap: anywhere;
		color: var(--text-secondary);
	}

	.legs .numeric {
		flex: none;
	}

	.acts {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-1);
		margin-inline: calc(var(--space-3) * -1);
	}

	.clear {
		gap: 0.375rem;
		margin-left: auto;
		color: var(--danger);
	}

	.clear :global(svg) {
		width: 1.125rem;
		height: 1.125rem;
	}
</style>
