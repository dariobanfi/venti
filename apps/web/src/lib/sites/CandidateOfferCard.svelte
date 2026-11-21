<!--
	The offer to add a place pilots take off from, for a pilot who took off
	there: the core's sentence, "Add this place", which opens the form of
	adding a site on the core's draft, and "Not this one", which sets the
	place aside in this browser. On a candidate's sheet and on Today.
-->
<script lang="ts">
	import type { CandidateOffer } from '#lib/core/index.ts';

	interface Props {
		offer: CandidateOffer;
		onadd: (offer: CandidateOffer) => void;
		ondismiss: (offer: CandidateOffer) => void;
	}

	let { offer, onadd, ondismiss }: Props = $props();
</script>

<div class="card" role="group" aria-label={offer.addWords}>
	<p class="row said">{offer.words}</p>
	<p class="row said lesser">{offer.flight.words}</p>
	<button class="row link" type="button" onclick={() => onadd(offer)}>{offer.addWords}</button>
	<button class="row link" type="button" onclick={() => ondismiss(offer)}>
		{offer.dismissWords}
	</button>
</div>

<style>
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

	.lesser {
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
</style>
