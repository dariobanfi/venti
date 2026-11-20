<!--
	Other pilots' flights as a list: everyone's, or one pilot's. More is
	fetched with a button at its foot, and under it stands whose flights
	these are and whose reckoning the distances and points.
-->
<script lang="ts">
	import Icon from '#lib/design/Icon.svelte';
	import type { ListState } from './pages';
	import PublicFlightRow from './PublicFlightRow.svelte';

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
	}

	let { label, list, named, nothing, reckoning, onmore, onagain }: Props = $props();
</script>

{#if list.flights.length > 0}
	<div class="card">
		<!-- What the columns of a wide screen hold. Each number says so itself
		     to a screen reader, so the headings are for the eye. -->
		<div class="columns" aria-hidden="true">
			<span>Distance</span>
			<span>Airtime</span>
			<span>Points</span>
			<span>Highest</span>
		</div>
		<ul aria-label={label}>
			{#each list.flights as flight (flight.id)}
				<PublicFlightRow {flight} {named} />
			{/each}
		</ul>
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
		/* The width of one column of numbers on a wide screen. */
		--column: 6.5rem;
	}

	.columns {
		display: none;
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

	@media (min-width: 56rem) {
		.columns {
			display: flex;
			justify-content: flex-end;
			padding: 0.625rem 1rem 0.375rem;
			font-size: var(--caption);
			color: var(--text-secondary);
			border-bottom: 0.5px solid var(--separator);
		}

		.columns span {
			width: var(--column);
			text-align: right;
		}
	}
</style>
