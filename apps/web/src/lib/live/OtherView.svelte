<!--
	One aircraft another network hears (the Open Glider Network): what kind
	it is, its name where its owner allows one, the core's line about it,
	how old its position is, and whose data it is. The network gives
	positions only: there is no flight to follow.
-->
<script lang="ts">
	import type { Credit, OtherAircraftView } from '#lib/core/index.ts';

	interface Props {
		aircraft: OtherAircraftView;
		/** Whose data it is: the network's credit, with its notice. */
		credit: Credit | undefined;
	}

	let { aircraft, credit }: Props = $props();
</script>

<div class="other">
	<div class="card">
		{#if aircraft.name}
			<p class="row name">{aircraft.name}</p>
		{/if}
		<!-- The core's line ends with how old the position is. -->
		<p class="row" data-other-line>{aircraft.line}</p>
	</div>
	{#if credit}
		<p class="footnote">
			{#if credit.url}<a href={credit.url} rel="noopener noreferrer" target="_blank">
					{credit.text || credit.provider}
				</a>{:else}{credit.text || credit.provider}{/if}
			{#if credit.notice}<span>{credit.notice}</span>{/if}
		</p>
	{/if}
</div>

<style>
	.other {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}

	.card {
		background: var(--surface);
		border-radius: var(--radius-card);
		overflow: hidden;
	}

	.row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		min-height: var(--target);
		padding: 0.5rem 1rem;
	}

	/* A hairline between rows. */
	.row + .row {
		box-shadow: inset 0 0.5px 0 var(--separator);
	}

	.name {
		font-weight: 600;
	}

	.footnote {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		padding: 0 1rem;
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	.footnote a {
		color: var(--link);
	}
</style>
