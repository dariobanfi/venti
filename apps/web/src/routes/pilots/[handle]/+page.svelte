<!--
	One pilot's public flights, the newest first. Which pilot is in the
	address: `/pilots/<handle>/`.

	Above the flights stands what they add up to, as the core gives it
	(`PilotTotals.svelte`): on a wide screen in a column beside the table.
	The head has the name and handle, and the pilot's place and points in
	the newest season's ranking.
-->
<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { searchIn, withSearch } from '#lib/community/flight-search.ts';
	import SearchFilter from '#lib/community/SearchFilter.svelte';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { everyoneHref, pilotHref } from '#lib/app/routes.ts';
	import { NOT_RANKED } from '#lib/community/model.svelte.ts';
	import PilotTotals from '#lib/community/PilotTotals.svelte';
	import { format } from '#lib/design/format.ts';
	import { NOT_LISTED, queryOf } from '#lib/community/pages.ts';
	import PublicFlightList from '#lib/community/PublicFlightList.svelte';
	import { reportedPicture, reportedPilot } from '#lib/community/report.ts';
	import PilotFace from '#lib/community/PilotFace.svelte';
	import ReportSheet from '#lib/community/ReportSheet.svelte';
	import { nothingListed, PILOTS_RECKONING } from '#lib/community/words.ts';
	import Icon from '#lib/design/Icon.svelte';
	import FollowBar from '#lib/social/FollowBar.svelte';

	const app = getApp();
	const everyone = $derived(app.everyone);

	/** The pilot asked for, by handle. */
	const handle = $derived(page.params.handle ?? '');
	const pilot = $derived(everyone?.pilot?.handle === handle ? everyone?.pilot : undefined);
	const list = $derived(pilot?.list ?? NOT_LISTED);

	/** What the pilot's flights are searched by, as the address says. */
	const search = $derived(searchIn(page.url.searchParams));

	$effect(() => {
		if (handle) everyone?.showPilot(handle, search);
	});

	/** Searches the pilot's flights by something else: the address changes in place. */
	function find(next: typeof search): void {
		void goto(withSearch(pilotHref(handle), next), {
			replaceState: true,
			keepFocus: true,
			noScroll: true
		});
	}

	/** The pilot's name as the server has it. What another pilot wrote is shown as text. */
	const name = $derived(pilot?.name ?? (handle ? `@${handle}` : 'Pilot'));
	/** The pilot's picture, from their totals or else from one of their flights. */
	const picture = $derived(pilot?.statistics?.picturePath ?? list.flights[0]?.pilotPicture);
	/** Which form of report is open: about the pilot's page, or their picture. */
	let reporting = $state<'pilot' | 'picture'>();
	const pictureReport = $derived(app.said?.picture.report ?? 'Report picture');

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
			<div class="named">
				{#if pilot?.name}
					<PilotFace {picture} name={pilot.name} size="l" />
				{/if}
				<div class="names">
					<h1>{name}</h1>
					{#if pilot?.name && handle}
						<p>@{handle}</p>
					{/if}
				</div>
			</div>
			<!-- Who follows this pilot, and for a signed-in pilot the button. -->
			{#if pilot?.name && handle}
				<FollowBar {handle} />
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
					<PilotTotals
						statistics={pilot.statistics}
						onseason={(season) => everyone?.narrowPilot(season)}
					/>
				</aside>
			{:else if pilot?.statisticsProblem && list.flights.length > 0}
				<aside class="span-3 side" aria-label="What the flights add up to">
					<p class="said" role="alert">{pilot.statisticsProblem}</p>
					<button class="button-plain" type="button" onclick={() => everyone?.addUpAgain()}>
						{app.said?.common.tryAgain}
					</button>
				</aside>
			{/if}
			<div
				class={pilot?.statistics || (pilot?.statisticsProblem && list.flights.length > 0)
					? 'span-9'
					: 'span-12'}
			>
				<div class="asked">
					<SearchFilter {search} onsearch={find} />
				</div>
				<PublicFlightList
					label="Flights of {name}"
					{list}
					named={false}
					nothing={nothingListed(list, { from: '', to: '', search })}
					reckoning={PILOTS_RECKONING}
					onmore={() => everyone?.morePilot()}
					onagain={() => everyone?.againPilot()}
				/>
			</div>
		</div>
		<!-- A page the server has can be reported to whoever runs the server. -->
		{#if pilot?.name && handle}
			<div class="report">
				<button class="button-plain" type="button" onclick={() => (reporting = 'pilot')}>
					Report this pilot
				</button>
				{#if picture}
					<button class="button-plain" type="button" onclick={() => (reporting = 'picture')}>
						{pictureReport}
					</button>
				{/if}
			</div>
			<ReportSheet
				open={reporting === 'pilot'}
				{...reportedPilot(handle)}
				onclose={() => (reporting = undefined)}
			/>
			<ReportSheet
				open={reporting === 'picture'}
				{...reportedPicture(handle, pictureReport)}
				onclose={() => (reporting = undefined)}
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

	/* What the flights are searched by, over the list. */
	.asked {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2) var(--space-4);
		margin-bottom: var(--space-3);
	}

	/* The pilot's face beside the name and handle. */
	.named {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		min-width: 0;
	}

	.names {
		min-width: 0;
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
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
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
