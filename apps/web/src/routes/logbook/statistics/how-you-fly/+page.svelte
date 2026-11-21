<!--
	How you fly: what the pilot's own tracks say about how they fly, worked
	out by the core (`tendencies`) and reached from the foot of the
	statistics. Each observation is a card with its title, its words, a thin
	bar where the core gives a share, and a link to the flight that shows it
	most. The words are the core's; nothing here is coloured as good or bad,
	since these are observations and not a verdict.

	On a phone the cards follow each other; on a wide screen they stand in
	columns, in the density of the statistics.
-->
<script lang="ts">
	import { getApp } from '#lib/app/app.svelte.ts';
	import { flightHref, statisticsHref } from '#lib/app/routes.ts';
	import type { Tendencies } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import { barPercent, COUNTING, NOTHING_YET, SHOWS_IT_MOST } from '#lib/logbook/tendencies.ts';

	const app = getApp();
	const text = format();

	let tendencies = $state.raw<Tendencies>();
	let problem = $state<string>();

	// Counted by the core off the page, and again whenever the logbook
	// changes. An older logbook is counted flight by flight the first time.
	$effect(() => {
		const logbook = app.logbook;
		if (!logbook?.view) return;
		let current = true;
		logbook.tendencies().then(
			(found) => {
				if (!current) return;
				tendencies = found;
				problem = undefined;
			},
			(error: unknown) => {
				if (current) problem = error instanceof Error ? error.message : String(error);
			}
		);
		return () => {
			current = false;
		};
	});
</script>

<svelte:head>
	<title>How you fly · Venti</title>
</svelte:head>

<div class="page">
	<header>
		<a class="back" href={statisticsHref()}>
			<Icon name="back" />
			Statistics
		</a>
		<h1>How you fly</h1>
	</header>

	{#if tendencies}
		{#if tendencies.observations.length > 0}
			<ul class="cards">
				{#each tendencies.observations as observation (observation.kind)}
					<li>
						<div class="card" role="group" aria-labelledby="tendency-{observation.kind}">
							<h2 id="tendency-{observation.kind}">{observation.title}</h2>
							<p>{observation.words}</p>
							{#if observation.share !== undefined}
								<!-- The words say what the share is of; the bar only shows it. -->
								<span class="track" aria-hidden="true">
									<span class="bar" style:width="{barPercent(observation.share)}%"></span>
								</span>
							{/if}
							{#if observation.flightId}
								<a class="flight" href={flightHref(observation.flightId)}>
									<span class="pair">
										<span>{SHOWS_IT_MOST}</span>
										{#if observation.flightUnixS !== undefined}
											<span class="lesser">{text.day(observation.flightUnixS)}</span>
										{/if}
									</span>
									<span class="chevron"><Icon name="forward" /></span>
								</a>
							{/if}
						</div>
					</li>
				{/each}
			</ul>
		{:else}
			<p class="nothing">{NOTHING_YET}</p>
		{/if}
		<p class="footnote">{tendencies.note}</p>
	{:else if problem}
		<p class="nothing" role="alert">{problem}</p>
	{:else}
		<p class="nothing" role="status">{COUNTING}</p>
	{/if}
</div>

<style>
	.page {
		max-width: 44rem;
		margin: 0 auto;
		padding: calc(0.5rem + var(--safe-top)) calc(var(--gutter) + var(--safe-right))
			calc(1.5rem + var(--nav-bottom)) calc(var(--gutter) + var(--safe-left));
	}

	.back {
		display: inline-flex;
		align-items: center;
		gap: 0.125rem;
		min-height: var(--target);
		margin-left: -0.5rem;
		padding: 0 0.75rem 0 0.25rem;
		border-radius: var(--radius-control);
		color: var(--link);
	}

	h1 {
		margin-bottom: 0.75rem;
		font-size: var(--large-title);
		font-weight: 600;
	}

	.cards {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.card {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-4);
		background: var(--surface);
		border-radius: var(--radius-card);
	}

	h2 {
		font-size: var(--subheadline);
		font-weight: 600;
		color: var(--text-secondary);
	}

	/* A thin bar in the primary colour: a share, never good or bad. */
	.track {
		display: block;
		height: 0.25rem;
		border-radius: var(--radius-pill);
		background: var(--fill);
		overflow: hidden;
	}

	.bar {
		display: block;
		height: 100%;
		background: var(--accent);
	}

	.flight {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		margin: 0 calc(var(--space-2) * -1) calc(var(--space-2) * -1);
		padding: var(--space-2);
		border-radius: var(--radius-s);
		color: var(--link);
	}

	.pair {
		flex: 1;
		display: flex;
		flex-direction: column;
	}

	.lesser {
		font-size: var(--subheadline);
		color: var(--text-secondary);
	}

	.chevron {
		color: var(--icon-quiet);
		font-size: 0.75rem;
	}

	@media (hover: hover) {
		.flight:hover,
		.back:hover {
			background: var(--fill);
		}
	}

	.footnote {
		margin-top: var(--space-4);
		padding: 0 1rem;
		font-size: var(--caption);
		color: var(--text-secondary);
	}

	.nothing {
		margin-top: var(--space-6);
		color: var(--text-secondary);
		text-align: center;
	}

	/* Wide: the cards in columns, as dense as the statistics. */
	@media (min-width: 56rem) {
		.page {
			max-width: 80rem;
			padding-top: var(--space-4);
		}

		.cards {
			display: grid;
			grid-template-columns: repeat(auto-fill, minmax(20rem, 1fr));
			align-items: start;
		}

		.card {
			padding: var(--space-3);
			border: 1px solid var(--color-border);
			border-radius: var(--radius-m);
		}
	}
</style>
