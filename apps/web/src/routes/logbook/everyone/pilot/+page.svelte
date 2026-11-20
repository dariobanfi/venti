<!--
	One pilot's public flights, the newest first. Which pilot is in the
	address: `?handle=`.
-->
<script lang="ts">
	import { browser } from '$app/env';
	import { page } from '$app/state';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { everyoneHref } from '#lib/app/routes.ts';
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

	/** Back to everyone's flights as they were asked for. */
	const back = $derived(everyoneHref(everyone ? queryOf(everyone.asked) : ''));
</script>

<svelte:head>
	<title>{name} · Venti</title>
</svelte:head>

<div class="page">
	<a class="back" href={back}>
		<Icon name="back" />
		Everyone
	</a>
	<header>
		<h1>{name}</h1>
		{#if pilot?.name && handle}
			<p>@{handle}</p>
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
		<PublicFlightList
			label="Flights of {name}"
			{list}
			named={false}
			nothing={nothingListed(list, { from: '', to: '' })}
			reckoning={PILOTS_RECKONING}
			onmore={() => everyone?.morePilot()}
			onagain={() => everyone?.againPilot()}
		/>
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
	.page {
		display: flex;
		flex-direction: column;
		min-height: 100dvh;
		max-width: 60rem;
		margin: 0 auto;
		padding: calc(0.75rem + var(--safe-top)) calc(var(--gutter) + var(--safe-right))
			calc(1.5rem + var(--nav-bottom)) calc(var(--gutter) + var(--safe-left));
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
		.page {
			padding-top: 1.5rem;
		}
	}
</style>
