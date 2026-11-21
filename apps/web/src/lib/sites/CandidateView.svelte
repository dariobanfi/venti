<!--
	One place where pilots take off and no site stands, as its small sheet
	on the map shows it under the core's title ("Pilots take off here"):
	the core's line, its height and its season, and where the core offers
	it to this pilot, the offer to add it. Every word is the core's.
-->
<script lang="ts">
	import type { CandidateOffer, SiteCandidateView } from '#lib/core/index.ts';
	import CandidateOfferCard from './CandidateOfferCard.svelte';

	interface Props {
		candidate: SiteCandidateView;
		offer?: CandidateOffer;
		/** Under what the flights it is made from are given, in the server's words. */
		terms?: string;
		onadd: (offer: CandidateOffer) => void;
		ondismiss: (offer: CandidateOffer) => void;
	}

	let { candidate, offer, terms = '', onadd, ondismiss }: Props = $props();
</script>

<div class="candidate">
	<div class="card">
		<p class="row said">{candidate.words}</p>
		<p class="row said numeric">{candidate.altitudeWords}</p>
		{#if candidate.seasonWords}
			<p class="row said">{candidate.seasonWords}</p>
		{/if}
	</div>
	{#if offer}
		<CandidateOfferCard {offer} {onadd} {ondismiss} />
	{/if}
	{#if terms}
		<p class="footnote">{terms}</p>
	{/if}
</div>

<style>
	.candidate {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}

	.card {
		background: var(--surface);
		border-radius: var(--radius-card);
		overflow: hidden;
	}

	.row {
		display: flex;
		align-items: center;
		min-height: var(--target);
		padding: var(--space-2) var(--space-4);
		overflow-wrap: anywhere;
	}

	.row + .row {
		box-shadow: inset 0 0.5px 0 var(--separator);
	}

	.footnote {
		padding: 0 var(--space-4);
		font-size: var(--footnote);
		color: var(--text-secondary);
		overflow-wrap: anywhere;
	}
</style>
