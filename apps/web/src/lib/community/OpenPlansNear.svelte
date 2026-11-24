<!--
	The open plans near a place, for Community: the plans for everyone
	flying from the area for the coming days, each a card
	(`OpenPlanCard.svelte`), in the core's order, by day and time. The
	place is Community's (`near-place.svelte.ts`), the same as the pilots
	near; the radius and the days are the core's and the server's. The empty
	list says the core's sentence.
-->
<script lang="ts">
	import { getApp } from '#lib/app/app.svelte.ts';
	import type { Coordinate } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import { dayTitle } from '#lib/groups/days.ts';
	import type { OpenPlansModel } from './open-plans.svelte';
	import OpenPlanCard from './OpenPlanCard.svelte';

	interface Props {
		model: OpenPlansModel;
		/** Where the list is for. */
		place: Coordinate;
		/** The id of the section's heading, for the list's name. */
		labelledby: string;
		/** The name the pilot of this browser is shown by, for their face. */
		myName: string | undefined;
	}

	let { model, place, labelledby, myName }: Props = $props();

	const app = getApp();
	const text = format();

	$effect(() => model.keepNear(place));

	const near = $derived(model.near);
	/** Every plan in its day's order, with what its day is called. */
	const plans = $derived(
		(near?.days ?? []).flatMap((day) =>
			day.plans.map((plan) => ({ plan, day: dayTitle(day.day, model.today, text) }))
		)
	);
</script>

{#if near}
	{#if model.problem}
		<!-- The list is from before: the server could not be asked again. -->
		<p class="quiet" role="status">{model.problem}</p>
	{/if}
	{#if plans.length === 0}
		<p class="quiet">{near.emptyWords}</p>
	{:else}
		<ul class="cards" aria-labelledby={labelledby}>
			{#each plans as { plan, day } (plan.id)}
				<OpenPlanCard {plan} {day} {model} {myName} />
			{/each}
		</ul>
	{/if}
	{#if near.isCutShort}
		<p class="note">There are more open plans near here: the later days are missing.</p>
	{/if}
{:else if model.problem}
	<p class="quiet" role="alert">{model.problem}</p>
	<button class="button-plain again" type="button" onclick={() => model.load()}>
		{app.said?.common.tryAgain}
	</button>
{:else}
	<p class="quiet" role="status">Loading…</p>
{/if}

<style>
	.quiet {
		color: var(--text-secondary);
	}

	.note {
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	.again {
		align-self: flex-start;
		margin-left: calc(-1 * var(--space-3));
	}

	/* Cards side by side as far as the width has room, one under the
	   other on a phone. */
	.cards {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(min(100%, 15rem), 1fr));
		gap: var(--space-3);
		list-style: none;
	}
</style>
