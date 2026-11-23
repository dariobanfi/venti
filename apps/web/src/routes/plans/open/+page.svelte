<!--
	One open plan, for everyone flying from its area: everything it says,
	who posted it, who is in, and the pilot's own word on it, "I'm in" or
	"I'm out". Its author changes and deletes it; another pilot who is
	signed in reports it. Signed out, it reads the same and "I'm in" leads
	to the account. Which plan is in the address: `?id=`.

	The plan is read from the server when the page opens and after every
	change; a plan whose day has passed by two days, or that was deleted,
	is no longer there, and the page says so in the core's words.
-->
<script lang="ts">
	import { browser } from '$app/env';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { accountHref, communityHref, mapHref, pilotHref } from '#lib/app/routes.ts';
	import OpenPlanForm from '#lib/community/OpenPlanForm.svelte';
	import { reportedOpenPlan } from '#lib/community/report.ts';
	import ReportSheet from '#lib/community/ReportSheet.svelte';
	import type { OpenPlanView } from '#lib/core/index.ts';
	import ConfirmDialog from '#lib/design/ConfirmDialog.svelte';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import Sheet from '#lib/design/Sheet.svelte';
	import ForecastView from '#lib/forecast/ForecastView.svelte';
	import { dayOf, dayTitle } from '#lib/groups/days.ts';

	const app = getApp();
	const text = format();
	const model = $derived(app.openPlans);
	const account = $derived(app.account);
	const signedIn = $derived(Boolean(account?.signedIn));

	/** The plan asked for. The address is not known while the pages are written out at build time. */
	const id = $derived(browser ? page.url.searchParams.get('id') : null);

	let plan = $state.raw<OpenPlanView>();
	/** Why the plan could not be read, in words. */
	let missing = $state.raw<string>();
	let working = $state(false);
	/** Why the last thing did not work, in words. */
	let problem = $state.raw<string>();
	let editing = $state(false);
	let confirmingDelete = $state(false);
	let forecasting = $state(false);
	let reporting = $state(false);

	// Read when the page opens, again after a change and when the pilot
	// signs in or out: whose it is and who is "You" follow the account.
	$effect(() => {
		const open = model;
		const asked = id;
		void model?.revision;
		void account?.signedIn;
		void account?.account?.handle;
		if (!open || !asked) return;
		let current = true;
		open.plan(asked).then(
			(read) => {
				if (!current) return;
				plan = read;
				missing = undefined;
			},
			(error) => {
				if (!current) return;
				plan = undefined;
				missing = error instanceof Error ? error.message : String(error);
			}
		);
		return () => {
			current = false;
		};
	});

	/** Does one thing on the server and says what went wrong, if anything. */
	async function act(work: () => Promise<unknown>, thenLeave = false) {
		if (working) return;
		working = true;
		problem = undefined;
		try {
			await work();
			if (thenLeave) await goto(communityHref());
		} catch (error) {
			problem = error instanceof Error ? error.message : String(error);
		} finally {
			working = false;
		}
	}

	async function setIn(shown: OpenPlanView) {
		await act(async () => {
			plan = await model!.setIn(shown.id, !shown.amIn);
		});
	}

	function deletePlan(shown: OpenPlanView) {
		confirmingDelete = false;
		void act(() => model!.delete(shown.id), true);
	}
</script>

<svelte:head>
	<title>Open plan · Venti</title>
</svelte:head>

<div class="settings-page">
	<header>
		<a class="back-link" href={communityHref()}>
			<Icon name="back" />
			Community
		</a>
	</header>
	<h1>Open plan</h1>

	{#if app.failure}
		<p class="group-note">Venti could not open its data. {app.failure}</p>
	{:else if account?.hasServer === false}
		<p class="lead">This copy of the app is served without Venti behind it.</p>
	{:else if plan && model}
		{@const shown = plan}
		<section class="group" aria-label="The plan">
			<dl class="group-card">
				<div class="group-row">
					<dt>Where</dt>
					<dd>{shown.place}</dd>
				</div>
				<div class="group-row">
					<dt>Day</dt>
					<dd>{dayTitle(shown.day, dayOf(new Date()), text)}</dd>
				</div>
				{#if shown.meetAt}
					<div class="group-row">
						<dt>Time</dt>
						<dd class="numeric">{shown.meetAt}</dd>
					</div>
				{/if}
				<div class="group-row">
					<dt>Offer</dt>
					<dd>{shown.offerWords}</dd>
				</div>
				<div class="group-row">
					<dt>Posted by</dt>
					<dd>
						{#if shown.isMine}You{:else}<a href={pilotHref(shown.authorHandle)}>{shown.author}</a
							>{/if}
					</dd>
				</div>
			</dl>
		</section>

		{#if shown.note}
			<section class="group" aria-labelledby="open-plan-note">
				<h2 class="group-title" id="open-plan-note">Note</h2>
				<div class="group-card">
					<p class="note">{shown.note}</p>
				</div>
			</section>
		{/if}

		<!-- Where the place is: on the map, and its forecast. -->
		<section class="group" aria-label="The place">
			<div class="group-card">
				<a class="group-row" href={mapHref(shown.position)}>
					On the map
					<Icon name="forward" />
				</a>
				<button class="group-row" type="button" onclick={() => (forecasting = true)}>
					Forecast for this place
					<Icon name="forward" />
				</button>
			</div>
			<p class="group-note numeric">{text.place(shown.position.lat, shown.position.lon)}</p>
		</section>
		<Sheet open={forecasting} title="Forecast" onclose={() => (forecasting = false)}>
			{#if forecasting}
				<ForecastView
					place={shown.position}
					name={shown.place}
					ask={(place) => app.forecast(place)}
				/>
			{/if}
		</Sheet>

		<section class="group" aria-labelledby="open-plan-in">
			<h2 class="group-title" id="open-plan-in">Who is in</h2>
			<div class="group-card">
				<ul aria-labelledby="open-plan-in">
					{#each shown.pilotsIn as name, index (index)}
						<li class="group-row">{name}</li>
					{:else}
						<li class="group-row nobody">{shown.inWords}</li>
					{/each}
				</ul>
				<!-- The pilot's own word on the plan. -->
				{#if signedIn}
					<button class="group-row" type="button" disabled={working} onclick={() => setIn(shown)}>
						{shown.amIn ? "I'm out" : "I'm in"}
						<Icon name={shown.amIn ? 'close' : 'check'} />
					</button>
				{:else}
					<a class="group-row" href={accountHref()}>
						I'm in
						<Icon name="forward" />
					</a>
				{/if}
			</div>
			{#if shown.publicWords && !shown.amIn}
				<p class="group-note">{shown.publicWords}</p>
			{/if}
		</section>

		{#if shown.isMine}
			<section class="group" aria-label="Changing the plan">
				<div class="group-card">
					<button
						class="group-row"
						type="button"
						disabled={working}
						onclick={() => (editing = true)}
					>
						Change the plan
					</button>
					<button
						class="group-row destructive"
						type="button"
						disabled={working}
						onclick={() => (confirmingDelete = true)}
					>
						Delete the plan
					</button>
				</div>
			</section>
		{:else if signedIn}
			<!-- Another pilot's plan can be reported to whoever runs the server. -->
			<section class="group" aria-label="Reporting the plan">
				<div class="group-card">
					<button class="group-row" type="button" onclick={() => (reporting = true)}>
						Report this plan
					</button>
				</div>
			</section>
		{/if}
		{#if problem}
			<p class="group-note refusal" role="alert">{problem}</p>
		{/if}

		{#if signedIn}
			<ReportSheet
				open={reporting}
				{...reportedOpenPlan(shown.id)}
				onclose={() => (reporting = false)}
			/>
		{/if}
		{#if shown.isMine}
			<OpenPlanForm
				open={editing}
				{model}
				near={shown.position}
				editing={shown}
				onclose={() => (editing = false)}
				onsaved={(saved) => (plan = saved)}
			/>
			<ConfirmDialog
				open={confirmingDelete}
				title="Delete this plan?"
				message="It goes for everyone, with who was in."
				confirm="Delete the plan"
				destructive
				onconfirm={() => deletePlan(shown)}
				oncancel={() => (confirmingDelete = false)}
			/>
		{/if}
	{:else if working}
		<!-- Deleted a moment ago: Community is next. -->
	{:else if missing}
		<p class="lead" role="alert">{missing}</p>
	{:else if id === null && browser}
		<p class="lead">This plan is no longer there.</p>
	{:else}
		<p class="lead" role="status">Asking Venti…</p>
	{/if}
</div>

<style>
	.lead {
		margin-top: 0.5rem;
		padding: 0 1rem;
		color: var(--text-secondary);
	}

	/* What the pilot typed keeps its lines. */
	.note {
		padding: 0.75rem 1rem;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}

	dd a {
		color: var(--link);
	}

	.nobody {
		color: var(--text-secondary);
	}

	.group-row :global(svg) {
		font-size: 0.875em;
	}

	.refusal {
		color: var(--danger);
	}
</style>
