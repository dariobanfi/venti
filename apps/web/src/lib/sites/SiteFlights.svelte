<!--
	What the public flights that began at a take-off say about it: a
	section of a site's page under its confirmations, titled by the core
	("Public flights from here").

	The core's count line, or its line for none; what is counted as its
	note; the twelve months as a small row of bars, each as tall as the
	core's share, in the manner of the statistics' bars (the primary
	colour: a count, never good or bad), the short names under them and
	the core's words for each to a screen reader; the season's sentence;
	the best flights as rows, each opening the flight as a public flight
	opens; and the terms' text at the foot in the caption style. Every
	word is the core's (`crates/core/src/takeoffs.rs`).

	Nothing at all for a site that is not a take-off. While the answer is
	read "Loading…", where it cannot be read the reason with "Try again",
	as the confirmations have them.
-->
<script lang="ts">
	import { getApp } from '#lib/app/app.svelte.ts';
	import { publicFlightHref } from '#lib/app/routes.ts';
	import type { SiteFlights } from '#lib/core/index.ts';
	import MonthBars from '#lib/design/MonthBars.svelte';
	import { said } from './edit.svelte';

	const app = getApp();

	interface Props {
		/** The site's identifier, of any layer. */
		id: string;
	}

	let { id }: Props = $props();

	const takeoffs = $derived(getApp().takeoffs);
	const words = $derived(takeoffs?.words);

	let flights = $state.raw<SiteFlights>();
	let problem = $state.raw<string>();
	/** Counts "Try again", after which the answer is read again. */
	let again = $state(0);

	$effect(() => {
		void again;
		const model = takeoffs;
		if (flights?.siteId !== id) flights = undefined;
		problem = undefined;
		if (!model) return;
		let current = true;
		model.site(id).then(
			(answer) => {
				if (current) flights = answer;
			},
			(error) => {
				if (current) problem = said(error);
			}
		);
		return () => {
			current = false;
		};
	});

	/** Whether the terms for reusing the flights are unfolded. */
	let termsOpen = $state(false);
	const uid = $props.id();

	/** The answer for this site, once it is read; nothing while another site's is held. */
	const shown = $derived(flights?.siteId === id ? flights : undefined);
</script>

{#if words && (!shown || shown.isTakeoff)}
	<section class="flights" aria-label={words.title}>
		<h3>{words.title}</h3>
		{#if shown}
			<div class="card">
				<p class="row said">{shown.words || shown.nobodyWords}</p>
				{#if shown.flights > 0}
					<div class="row months" role="group" aria-label={words.monthsTitle}>
						<p class="small">{words.monthsTitle}</p>
						<MonthBars months={shown.months} />
						{#if shown.seasonWords}
							<p class="season">{shown.seasonWords}</p>
						{/if}
					</div>
				{/if}
			</div>
			{#if shown.about}
				<p class="footnote">{shown.about}</p>
			{/if}
			{#if shown.best.length > 0}
				<h3>{words.bestTitle}</h3>
				<ul class="card best" aria-label={words.bestTitle}>
					{#each shown.best as best (best.id)}
						<li>
							<a class="row link" href={publicFlightHref(best.id)}>{best.words}</a>
						</li>
					{/each}
				</ul>
			{/if}
			{#if shown.terms.text && shown.terms.title && shown.flights > 0}
				<!-- The terms for reusing these flights: one short link, titled
				     by the core, that opens its words; only where there are flights. -->
				<button
					class="terms"
					type="button"
					aria-expanded={termsOpen}
					aria-controls="{uid}-terms"
					onclick={() => (termsOpen = !termsOpen)}
				>
					{shown.terms.title}
				</button>
				{#if termsOpen}
					<p class="footnote" id="{uid}-terms">{shown.terms.text}</p>
				{/if}
			{/if}
		{:else if problem}
			<div class="card">
				<p class="row said" role="alert">{problem}</p>
				<button class="row link" type="button" onclick={() => (again += 1)}
					>{app.said?.common.tryAgain}</button
				>
			</div>
		{:else}
			<p class="waiting" role="status">Loading…</p>
		{/if}
	</section>
{/if}

<style>
	.flights {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	h3 {
		padding: 0 var(--space-4);
		font-size: var(--subheadline);
		font-weight: 600;
		color: var(--text-secondary);
	}

	/* The terms' link: a line of the footnote's size in the link colour. */
	.terms {
		align-self: flex-start;
		min-height: var(--target);
		padding: 0 var(--space-4);
		color: var(--link);
		font-size: var(--footnote);
		text-align: left;
	}

	.card {
		background: var(--surface);
		border-radius: var(--radius-card);
		overflow: hidden;
		list-style: none;
	}

	.row {
		display: flex;
		align-items: center;
		gap: var(--space-4);
		min-height: var(--target);
		padding: var(--space-2) var(--space-4);
		overflow-wrap: anywhere;
	}

	.row + .row,
	li + li .row {
		box-shadow: inset 0 0.5px 0 var(--separator);
	}

	.months {
		flex-direction: column;
		align-items: stretch;
		gap: var(--space-2);
		padding-block: var(--space-3);
	}

	.small,
	.season {
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	.link {
		width: 100%;
		color: var(--link);
		text-align: left;
	}

	@media (hover: hover) {
		.link:hover {
			background: var(--fill);
		}
	}

	.link:focus-visible {
		outline-offset: -0.1875rem;
		border-radius: var(--radius-card);
	}

	.footnote {
		padding: 0 var(--space-4);
		font-size: var(--footnote);
		color: var(--text-secondary);
		overflow-wrap: anywhere;
	}

	.waiting {
		padding: var(--space-4);
		color: var(--text-secondary);
		text-align: center;
	}
</style>
