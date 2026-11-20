<!--
	One plan for a flying day: everything it says, who is in, and the pilot's
	own word on it, "I am in" or "I am out". Its author changes it; its author
	and the keeper of its group delete it. Which plan is in the address:
	`?id=`. The iPhone app's `PlanScreen` is its twin.

	The plan is read from the board, which is asked for when the screen
	opens: a plan whose day has passed, or that was deleted, is no longer
	there, and the screen says so.
-->
<script lang="ts">
	import { browser } from '$app/env';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { plansHref } from '#lib/app/routes.ts';
	import type { PlanView } from '#lib/core/index.ts';
	import ConfirmDialog from '#lib/design/ConfirmDialog.svelte';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import Sheet from '#lib/design/Sheet.svelte';
	import ForecastView from '#lib/forecast/ForecastView.svelte';
	import { dayTitle } from '#lib/groups/days.ts';
	import PlanForm from '#lib/groups/PlanForm.svelte';
	import WithAccount from '#lib/groups/WithAccount.svelte';
	import { question } from '#lib/groups/words.ts';

	const app = getApp();
	const text = format();
	const groups = $derived(app.groups);

	/** The plan asked for. The address is not known while the pages are written out at build time. */
	const id = $derived(browser ? page.url.searchParams.get('id') : null);

	$effect(() => groups?.keepCurrent());

	let working = $state(false);
	/** Why the last thing did not work, in words. */
	let problem = $state.raw<string>();
	let editing = $state(false);
	let confirmingDelete = $state(false);
	/** Whether the forecast for the plan's place is open. */
	let forecasting = $state(false);
	const asked = question({ about: 'deletePlan' });

	/** Does one thing on the server and says what went wrong, if anything. */
	async function act(work: () => Promise<void>, thenLeave = false) {
		if (working) return;
		working = true;
		problem = undefined;
		try {
			await work();
			if (thenLeave) await goto(plansHref());
		} catch (error) {
			problem = error instanceof Error ? error.message : String(error);
		} finally {
			working = false;
		}
	}

	function deletePlan(plan: PlanView) {
		confirmingDelete = false;
		void act(() => groups!.deletePlan(plan), true);
	}
</script>

<svelte:head>
	<title>Plan · Venti</title>
</svelte:head>

<div class="settings-page">
	<header>
		<a class="back-link" href={plansHref()}>
			<Icon name="back" />
			Flying plans
		</a>
	</header>
	<h1>Plan</h1>

	<WithAccount>
		{#snippet children(groups)}
			{@const plan = id ? groups.plan(id) : undefined}
			{#if plan}
				<section class="group" aria-label="The plan">
					<dl class="group-card">
						<div class="group-row">
							<dt>Where</dt>
							<dd>{plan.place}</dd>
						</div>
						<div class="group-row">
							<dt>Day</dt>
							<dd>{dayTitle(plan.day, groups.today, text)}</dd>
						</div>
						{#if plan.meetAt}
							<div class="group-row">
								<dt>Time</dt>
								<dd class="numeric">{plan.meetAt}</dd>
							</div>
						{/if}
						{#if plan.groupName}
							<div class="group-row">
								<dt>Group</dt>
								<dd>{plan.groupName}</dd>
							</div>
						{/if}
						<div class="group-row">
							<dt>Posted by</dt>
							<dd>{plan.isMine ? 'You' : plan.author}</dd>
						</div>
					</dl>
				</section>

				{#if plan.note}
					<section class="group" aria-labelledby="plan-note">
						<h2 class="group-title" id="plan-note">Note</h2>
						<div class="group-card">
							<p class="note">{plan.note}</p>
						</div>
					</section>
				{/if}

				{#if plan.position}
					{@const position = plan.position}
					<!-- The plan says where on the map: the forecast for that
					     place is one step away. -->
					<section class="group" aria-label="The place">
						<div class="group-card">
							<button class="group-row" type="button" onclick={() => (forecasting = true)}>
								Forecast for this place
								<Icon name="forward" />
							</button>
						</div>
						<p class="group-note numeric">{text.place(position.lat, position.lon)}</p>
					</section>
					<Sheet open={forecasting} title="Forecast" onclose={() => (forecasting = false)}>
						{#if forecasting}
							<ForecastView
								place={position}
								name={plan.place}
								ask={(place) => app.forecast(place)}
							/>
						{/if}
					</Sheet>
				{/if}

				<section class="group" aria-labelledby="plan-in">
					<h2 class="group-title" id="plan-in">Who is in</h2>
					<div class="group-card">
						<ul aria-labelledby="plan-in">
							{#each plan.pilotsIn as name, index (index)}
								<li class="group-row">{name}</li>
							{:else}
								<li class="group-row nobody">Nobody yet</li>
							{/each}
						</ul>
						<!-- The pilot's own word on the plan. -->
						<button
							class="group-row"
							type="button"
							disabled={working}
							onclick={() => act(() => groups.setIn(plan, !plan.amIn))}
						>
							{plan.amIn ? 'I am out' : 'I am in'}
							<Icon name={plan.amIn ? 'close' : 'check'} />
						</button>
					</div>
				</section>

				{#if plan.isMine || plan.canDelete}
					<section class="group" aria-label="Changing the plan">
						<div class="group-card">
							{#if plan.isMine}
								<button
									class="group-row"
									type="button"
									disabled={working}
									onclick={() => (editing = true)}
								>
									Change the plan
								</button>
							{/if}
							{#if plan.canDelete}
								<button
									class="group-row destructive"
									type="button"
									disabled={working}
									onclick={() => (confirmingDelete = true)}
								>
									Delete the plan
								</button>
							{/if}
						</div>
					</section>
				{/if}
				{#if problem}
					<p class="group-note refusal" role="alert">{problem}</p>
				{/if}

				<PlanForm open={editing} {groups} editing={plan} onclose={() => (editing = false)} />
				<ConfirmDialog
					open={confirmingDelete}
					{...asked}
					onconfirm={() => deletePlan(plan)}
					oncancel={() => (confirmingDelete = false)}
				/>
			{:else if working}
				<!-- Deleted a moment ago: the board is next. -->
			{:else if groups.board}
				<p class="lead">This plan is no longer there.</p>
			{:else if groups.problem}
				<p class="lead" role="alert">{groups.problem}</p>
			{:else}
				<p class="lead" role="status">Asking the server…</p>
			{/if}
		{/snippet}
	</WithAccount>
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
