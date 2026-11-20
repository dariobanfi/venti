<!--
	Flying plans: who means to fly where on which day, among pilots who know
	each other. The board lists the plans of every group the pilot is in, day
	by day from the pilot's own today on, and leads to each plan, to a form
	for a new one, and to the groups. The iPhone app's `PlansSheet` is its
	twin.

	Nothing notifies: the board is asked for when this screen opens, after
	every change, and when the tab comes back to the front.
-->
<script lang="ts">
	import { getApp } from '#lib/app/app.svelte.ts';
	import { groupsHref } from '#lib/app/routes.ts';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import { dayTitle } from '#lib/groups/days.ts';
	import PlanForm from '#lib/groups/PlanForm.svelte';
	import PlanRow from '#lib/groups/PlanRow.svelte';
	import WithAccount from '#lib/groups/WithAccount.svelte';
	import { CUT_SHORT, NO_GROUP, NO_PLANS, WHO_SEES_A_PLAN } from '#lib/groups/words.ts';

	const app = getApp();
	const text = format();
	const groups = $derived(app.groups);

	$effect(() => groups?.keepCurrent());

	/** Whether the form for a new plan is open. */
	let adding = $state(false);
	/** A plan needs a group to be posted to. */
	const canPlan = $derived(
		Boolean(app.account?.signedIn) && (groups?.board?.groups.length ?? 0) > 0
	);
</script>

<svelte:head>
	<title>Flying plans · Venti</title>
</svelte:head>

<div class="settings-page">
	<header></header>
	<div class="title">
		<h1>Flying plans</h1>
		{#if canPlan}
			<button class="new" type="button" onclick={() => (adding = true)}>
				<Icon name="plus" />
				<span>New plan</span>
			</button>
		{/if}
	</div>

	<WithAccount>
		{#snippet children(groups)}
			{@const board = groups.board}
			{#if board}
				{#if groups.problem}
					<!-- The board is from before: the server could not be asked again. -->
					<p class="lead" role="status">{groups.problem}</p>
				{/if}
				{#if board.groups.length === 0}
					<p class="lead">{NO_GROUP}</p>
				{:else if board.days.length === 0}
					<p class="lead">{NO_PLANS}</p>
				{/if}

				{#each board.days as day (day.day)}
					<section class="group" aria-labelledby="plans-{day.day}">
						<h2 class="group-title" id="plans-{day.day}">
							{dayTitle(day.day, groups.today, text)}
						</h2>
						<ul class="group-card" aria-labelledby="plans-{day.day}">
							{#each day.plans as plan (plan.id)}
								<li><PlanRow {plan} /></li>
							{/each}
						</ul>
					</section>
				{/each}
				{#if board.isCutShort}
					<p class="group-note">{CUT_SHORT}</p>
				{/if}

				<section class="group" aria-label="Groups">
					<div class="group-card">
						<a class="group-row" href={groupsHref()}>
							<span>{board.groups.length === 0 ? 'Make or join a group' : 'Groups'}</span>
							<span class="trailing">
								{#if board.groups.length > 0}
									<span class="group-value numeric">{text.count(board.groups.length)}</span>
								{/if}
								<span class="chevron"><Icon name="forward" /></span>
							</span>
						</a>
					</div>
					<p class="group-note">{WHO_SEES_A_PLAN}</p>
				</section>
			{:else if groups.problem}
				<p class="lead" role="alert">{groups.problem}</p>
				<button class="button-plain again" type="button" onclick={() => groups.load()}>
					Ask again
				</button>
			{:else}
				<p class="lead" role="status">Asking the server…</p>
			{/if}
		{/snippet}
	</WithAccount>
</div>

{#if groups}
	<PlanForm open={adding} {groups} onclose={() => (adding = false)} />
{/if}

<style>
	.title {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 1rem;
	}

	/* Narrow: a round button with a plus, as in the iPhone's toolbar. */
	.new {
		flex: none;
		display: grid;
		place-items: center;
		width: var(--target);
		height: var(--target);
		border-radius: 50%;
		background: var(--surface);
		color: var(--accent);
		box-shadow: var(--shadow-float);
	}

	.new span {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip-path: inset(50%);
		white-space: nowrap;
	}

	.lead {
		margin-top: 0.5rem;
		padding: 0 1rem;
		color: var(--text-secondary);
	}

	.again {
		margin: 0.25rem 0 0 0.25rem;
	}

	.trailing {
		display: flex;
		align-items: center;
		gap: 0.375rem;
		min-width: 0;
	}

	.chevron {
		color: var(--icon-quiet);
		font-size: 0.75rem;
	}

	/* Wide: the button has room to say what it does. */
	@media (min-width: 56rem) {
		.new {
			display: inline-flex;
			gap: 0.375rem;
			width: auto;
			padding: 0 1.125rem 0 0.875rem;
			border-radius: 999px;
			background: var(--accent);
			color: var(--on-accent);
			font-weight: 600;
			box-shadow: none;
		}

		.new span {
			position: static;
			width: auto;
			height: auto;
			clip-path: none;
		}
	}
</style>
