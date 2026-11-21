<!--
	One pilot's public flights, the newest first. Which pilot is in the
	address: `?handle=`.

	Above the flights stands what they add up to, as the core gives it
	(`PilotTotals.svelte`): on a wide screen in a column beside the table.
	The head has the name and handle, and the pilot's place and points in
	the newest season's ranking.
-->
<script lang="ts">
	import { browser } from '$app/env';
	import { page } from '$app/state';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { everyoneHref } from '#lib/app/routes.ts';
	import { NOT_RANKED } from '#lib/community/model.svelte.ts';
	import PilotTotals from '#lib/community/PilotTotals.svelte';
	import { format } from '#lib/design/format.ts';
	import { NOT_LISTED, queryOf } from '#lib/community/pages.ts';
	import PublicFlightList from '#lib/community/PublicFlightList.svelte';
	import { reportedPilot } from '#lib/community/report.ts';
	import ReportSheet from '#lib/community/ReportSheet.svelte';
	import { nothingListed, PILOTS_RECKONING } from '#lib/community/words.ts';
	import Icon from '#lib/design/Icon.svelte';

	const app = getApp();
	const everyone = $derived(app.everyone);

	/** The pilot asked for. The address is not known while the pages are written out at build time. */
	const handle = $derived(browser ? (page.url.searchParams.get('handle') ?? '') : undefined);
	const pilot = $derived(everyone?.pilot?.handle === handle ? everyone?.pilot : undefined);
	const list = $derived(pilot?.list ?? NOT_LISTED);

	$effect(() => {
		if (handle) everyone?.showPilot(handle);
	});

	/** The pilot's name as the server has it. What another pilot wrote is shown as text. */
	const name = $derived(pilot?.name ?? (handle ? `@${handle}` : 'Pilot'));
	/** Whether the form that reports the pilot's page is open. */
	let reporting = $state(false);

	// The pilot's place in the newest season, for the head.
	$effect(() => everyone?.showRanking(''));
	const ranking = $derived(everyone?.ranking ?? NOT_RANKED);
	const ranked = $derived(ranking.pilots.find((one) => one.handle === handle));
	const text = format();

	/** Back to everyone's flights as they were asked for. */
	const back = $derived(everyoneHref(everyone ? queryOf(everyone.asked) : ''));
</script>

<svelte:head>
	<title>{name} · Venti</title>
</svelte:head>

<div class="desk page">
	<a class="back" href={back}>
		<Icon name="back" />
		Everyone
	</a>
	<header>
		<div class="who">
			<h1>{name}</h1>
			{#if pilot?.name && handle}
				<p>@{handle}</p>
			{/if}
		</div>
		{#if ranked}
			<dl class="totals numeric">
				<div>
					<dt>Place in {ranking.season}</dt>
					<dd>{ranked.place}</dd>
				</div>
				<div>
					<dt>Points in {ranking.season}</dt>
					<dd>{text.points(ranked.points)}</dd>
				</div>
			</dl>
		{/if}
	</header>

	{#if handle === ''}
		<div class="unavailable">
			<span class="symbol"><Icon name="warning" /></span>
			<h2>This address names no pilot</h2>
			<p>A pilot’s flights are reached from a pilot’s name in the list of everyone’s flights.</p>
		</div>
	{:else if app.failure}
		<div class="unavailable">
			<span class="symbol"><Icon name="warning" /></span>
			<h2>Venti could not open its data</h2>
			<p>{app.failure}</p>
		</div>
	{:else}
		<div class="columns">
			{#if pilot?.statistics}
				<aside class="span-3 side" aria-label="What the flights add up to">
					<PilotTotals statistics={pilot.statistics} flights={list.flights} />
				</aside>
			{:else if pilot?.statisticsProblem && list.flights.length > 0}
				<aside class="span-3 side" aria-label="What the flights add up to">
					<p class="said" role="alert">{pilot.statisticsProblem}</p>
					<button class="button-plain" type="button" onclick={() => everyone?.addUpAgain()}>
						Try again
					</button>
				</aside>
			{/if}
			<div
				class={pilot?.statistics || (pilot?.statisticsProblem && list.flights.length > 0)
					? 'span-9'
					: 'span-12'}
			>
				<PublicFlightList
					label="Flights of {name}"
					{list}
					named={false}
					nothing={nothingListed(list, { from: '', to: '' })}
					reckoning={PILOTS_RECKONING}
					onmore={() => everyone?.morePilot()}
					onagain={() => everyone?.againPilot()}
				/>
			</div>
		</div>
		<!-- A page the server has can be reported to whoever runs the server. -->
		{#if pilot?.name && handle}
			<div class="report">
				<button class="button-plain" type="button" onclick={() => (reporting = true)}>
					Report this pilot
				</button>
			</div>
			<ReportSheet
				open={reporting}
				{...reportedPilot(handle)}
				onclose={() => (reporting = false)}
			/>
		{/if}
	{/if}
</div>

<style>
	/* The `.desk`'s column, of the whole width on a wide screen. */
	.page {
		display: flex;
		flex-direction: column;
		min-height: calc(100dvh - var(--nav-top));
	}

	.back {
		align-self: flex-start;
		display: inline-flex;
		align-items: center;
		gap: 0.125rem;
		min-height: var(--target);
		margin-left: -0.375rem;
		padding-right: 0.5rem;
		border-radius: var(--radius-control);
		color: var(--link);
	}

	header {
		margin-bottom: 1rem;
	}

	h1 {
		font-size: var(--large-title);
		font-weight: 600;
		overflow-wrap: anywhere;
	}

	header p {
		color: var(--text-secondary);
	}

	/* The pilot's place in the newest season: in a row under the name on a
	   phone, at the right of the head on a wide screen. */
	.totals {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2) var(--space-5);
		margin-top: var(--space-3);
	}

	.totals dt {
		font-size: var(--caption);
		color: var(--text-secondary);
	}

	.totals dd {
		font-size: var(--title);
		font-weight: 600;
	}

	.said {
		padding: var(--space-2) var(--space-4);
		color: var(--text-secondary);
	}

	.report {
		margin-top: 1rem;
		padding: 0 0.25rem;
	}

	.unavailable {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 0.375rem;
		max-width: 24rem;
		margin: 0 auto;
		padding-bottom: 3rem;
		text-align: center;
	}

	.symbol {
		color: var(--text-secondary);
		font-size: 2.25rem;
		margin-bottom: 0.5rem;
	}

	.unavailable h2 {
		font-size: var(--title);
		font-weight: 600;
	}

	.unavailable p {
		color: var(--text-secondary);
	}

	@media (min-width: 56rem) {
		header {
			display: flex;
			flex-wrap: wrap;
			align-items: flex-end;
			justify-content: space-between;
			gap: var(--space-3) var(--space-6);
		}

		.totals {
			margin-top: 0;
			gap: var(--space-6);
		}

		/* The totals stand at the right of the table. */
		.side {
			order: 1;
		}
	}
</style>
