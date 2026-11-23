<!--
	Community: pilots among each other, in one place (docs/plan.md, after
	the owner's testing session of 2026-11-22, item 7). In this order: the
	plans of the pilot's groups for the coming days, each a card with its
	day, its place, the faces of who is in and "I am in"; the flights of
	the pilots they follow, each a card with the track's shape and the
	airtime and distance large; finding a pilot by name; and what the
	pilot was told. A wide screen has the plans across the page and the
	feed beside the search and the notifications. For everyone, the open
	plans near the pilot for the coming days (`OpenPlansNear.svelte`), after
	the groups' plans, with the form for a new one; and at the foot the
	pilots who fly near the pilot (`PilotsNear.svelte`). Both lists are for
	one place (`near-place.svelte.ts`).

	Signed out, the open plans, finding a pilot and the pilots near are
	there, with a sign-in for the rest.
	Every part asks for itself: the board as the plans' pages always have
	(`GroupsModel.keepCurrent`), the feed as the home page does, the search
	as everyone's flights do, the notifications when the page opens. No
	request is of this page's own.

	A plan's and a group's pages are still under `/plans/`; `/plans/`
	itself leads here.
-->
<script lang="ts">
	import PilotFace from '#lib/community/PilotFace.svelte';
	import PilotsNear from '#lib/community/PilotsNear.svelte';
	import { NearPlace } from '#lib/community/near-place.svelte.ts';
	import OpenPlanForm from '#lib/community/OpenPlanForm.svelte';
	import OpenPlansNear from '#lib/community/OpenPlansNear.svelte';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { LEFT_AS_IT_IS } from '#lib/social/unread.svelte.ts';
	import {
		accountHref,
		everyoneHref,
		groupsHref,
		notificationsHref,
		pilotHref,
		publicFlightHref
	} from '#lib/app/routes.ts';
	import { NOT_SEARCHED } from '#lib/community/search.ts';
	import type { Notifications } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import { dayTitle } from '#lib/groups/days.ts';
	import PlanCard from '#lib/groups/PlanCard.svelte';
	import PlanForm from '#lib/groups/PlanForm.svelte';
	import { CUT_SHORT, NO_GROUP, NO_PLANS, WHO_SEES_A_PLAN } from '#lib/groups/words.ts';
	import FeedCard from '#lib/social/FeedCard.svelte';

	const app = getApp();
	const text = format();
	const account = $derived(app.account);
	const signedIn = $derived(Boolean(account?.signedIn));
	const own = $derived(signedIn ? account?.account : undefined);

	/* The plans of the pilot's groups, from the pilot's own today on. */
	const groups = $derived(app.groups);
	$effect(() => groups?.keepCurrent());
	const board = $derived(signedIn ? groups?.board : undefined);
	/** Every plan of the board in its day's order, with what its day is called. */
	const plans = $derived(
		(board?.days ?? []).flatMap((day) =>
			day.plans.map((plan) => ({ plan, day: dayTitle(day.day, groups?.today ?? '', text) }))
		)
	);
	/** Whether the form for a new plan is open. */
	let adding = $state(false);
	/** A plan needs a group to be posted to. */
	const canPlan = $derived(signedIn && (board?.groups.length ?? 0) > 0);

	/* The flights of the pilots followed: the newest of them. */
	const social = $derived(app.social);
	const said = $derived(social?.words);
	$effect(() => social?.showFeed(own?.handle));
	const feed = $derived(social?.feed);
	const FEED_CARDS = 6;
	const followed = $derived((feed?.rows ?? []).slice(0, FEED_CARDS));

	/* Finding a pilot by name, for everyone. */
	const everyone = $derived(app.everyone);
	let who = $state('');
	$effect(() => everyone?.findPilots(who));
	const search = $derived(everyone?.search ?? NOT_SEARCHED);
	const answered = $derived(search.text !== '' && !search.waiting);

	/* What the pilot was told: the newest lines, the rest on its own page. */
	const TOLD_LINES = 5;
	let told = $state.raw<Notifications>();
	let toldProblem = $state.raw<string>();
	let askedTold = $state(0);
	$effect(() => {
		const core = social?.core;
		void askedTold;
		if (!core || !signedIn) return;
		let current = true;
		toldProblem = undefined;
		core.notifications().then(
			(answer) => {
				if (!current) return;
				told = answer;
				// Shown: the marker of what is new moves past it, the alarms' as it is.
				if (answer) void app.unread?.seen({ notifications: answer.cursor, alarms: LEFT_AS_IT_IS });
			},
			(error) => {
				const why = error instanceof Error ? error.message : String(error);
				if (current) toldProblem = [said?.notificationsFailed, why].filter(Boolean).join(' ');
			}
		);
		return () => {
			current = false;
		};
	});

	/* The open plans near the pilot, for everyone, and the pilots near: one place. */
	const near = new NearPlace();
	$effect(() => near.watch());
	const openPlans = $derived(app.openPlans);
	/** Whether the form for a new open plan is open. */
	let addingOpen = $state(false);

	const id = $props.id();
</script>

<!-- The open plans near the pilot, for everyone: after the groups' plans, or first. -->
{#snippet openPlansNear()}
	{#if openPlans}
		<section class="part span-12" aria-labelledby="{id}-open">
			<header>
				<h2 id="{id}-open">Open plans near</h2>
				{#if signedIn}
					<button class="more" type="button" onclick={() => (addingOpen = true)}>
						New open plan
					</button>
				{/if}
			</header>
			<OpenPlansNear
				model={openPlans}
				place={near.place}
				labelledby="{id}-open"
				myName={own?.displayName}
			/>
		</section>
	{/if}
{/snippet}

<svelte:head>
	<title>Community · Venti</title>
</svelte:head>

<div class="desk community">
	<header class="page-head">
		<h1>Community</h1>
		{#if canPlan}
			<div class="actions">
				<button class="new" type="button" onclick={() => (adding = true)}>
					<Icon name="plus" />
					<span>New plan</span>
				</button>
			</div>
		{/if}
	</header>

	{#if app.failure}
		<p class="quiet">Venti could not open its data. {app.failure}</p>
	{:else if account?.hasServer === false}
		<p class="quiet">This copy of the app is served without Venti behind it.</p>
	{:else if !account}
		<p class="quiet" role="status">Opening…</p>
	{:else}
		<div class="columns">
			{#if signedIn}
				<!-- The plans of the pilot's groups. -->
				<section class="part span-12" aria-labelledby="{id}-plans">
					<header>
						<h2 id="{id}-plans">Plans</h2>
						{#if board && board.groups.length > 0}
							<a class="more" href={groupsHref()}>
								Groups <span class="numeric">{text.count(board.groups.length)}</span>
							</a>
						{/if}
					</header>
					{#if board}
						{#if groups?.problem}
							<!-- The board is from before: the server could not be asked again. -->
							<p class="quiet" role="status">{groups.problem}</p>
						{/if}
						{#if board.groups.length === 0}
							<div class="group-card words">
								<p>{NO_GROUP}</p>
								<a class="button-plain" href={groupsHref()}>Make or join a group</a>
							</div>
						{:else if plans.length === 0}
							<p class="quiet">{NO_PLANS}</p>
						{:else}
							<ul class="cards plans" aria-label="Plans">
								{#each plans as { plan, day } (plan.id)}
									{#if groups}
										<PlanCard {plan} {day} {groups} myName={own?.displayName} />
									{/if}
								{/each}
							</ul>
						{/if}
						{#if board.isCutShort}
							<p class="note">{CUT_SHORT}</p>
						{/if}
						{#if board.groups.length > 0}
							<p class="note">{WHO_SEES_A_PLAN}</p>
						{/if}
					{:else if groups?.problem}
						<p class="quiet" role="alert">{groups.problem}</p>
						<button class="button-plain again" type="button" onclick={() => groups?.load()}>
							{app.said?.common.tryAgain ?? 'Try again'}
						</button>
					{:else}
						<p class="quiet" role="status">Loading…</p>
					{/if}
				</section>

				{@render openPlansNear()}

				<!-- The flights of the pilots followed. -->
				{#if said && feed}
					<section class="part span-8" aria-labelledby="{id}-following">
						<header>
							<h2 id="{id}-following">{said.feedFollowing}</h2>
							{#if followed.length > 0}
								<a class="more" href={everyoneHref('?feed=following')}>All</a>
							{/if}
						</header>
						{#if feed.problem && followed.length === 0}
							<p class="quiet">{feed.problem}</p>
							<button class="button-plain again" type="button" onclick={() => social?.againFeed()}>
								{app.said?.common.tryAgain ?? 'Try again'}
							</button>
						{:else if !feed.known}
							<p class="quiet" role="status">Loading…</p>
						{:else if followed.length === 0}
							<p class="quiet">{feed.emptyLine}</p>
						{:else}
							<ul class="cards feed" aria-label={said.feedFollowing}>
								{#each followed as row (row.flight.id)}
									<FeedCard {row} />
								{/each}
							</ul>
						{/if}
					</section>
				{/if}
			{:else}
				{@render openPlansNear()}

				<!-- Signed out: what needs an account, and the way to one. -->
				<section class="part span-8" aria-label="Sign in">
					<div class="group-card words sign-in">
						<p>
							With an account you plan flying days with your groups, follow pilots and see what they
							fly, and hear who cheered your flights.
						</p>
						<a class="button-prominent" href={accountHref()}>Sign in</a>
					</div>
				</section>
			{/if}

			<div class="side span-4">
				<!-- Finding a pilot, for everyone. -->
				<section class="part" aria-labelledby="{id}-find">
					<header><h2 id="{id}-find">Find a pilot</h2></header>
					<div class="find">
						<input
							class="text-field"
							type="search"
							aria-labelledby="{id}-find"
							aria-controls="{id}-found"
							placeholder={everyone?.pilotsWords?.searchPlaceholder ?? ''}
							autocomplete="off"
							bind:value={who}
						/>
						<div id="{id}-found">
							<p class="result" role="status">
								{#if answered}{search.problem ?? search.found?.words ?? ''}{/if}
							</p>
							{#if answered && search.found && search.found.pilots.length > 0}
								<ul class="group-card" aria-label="Pilots found">
									{#each search.found.pilots as pilot (pilot.handle)}
										<li>
											<a class="group-row" href={pilotHref(pilot.handle)}>
												<PilotFace picture={pilot.picturePath} name={pilot.name} />
												<span class="line">{pilot.words}</span>
												<span class="chevron"><Icon name="forward" /></span>
											</a>
										</li>
									{/each}
								</ul>
							{/if}
						</div>
					</div>
				</section>

				<!-- What the pilot was told. -->
				{#if signedIn && said}
					<section class="part" aria-labelledby="{id}-told">
						<header>
							<h2 id="{id}-told">{said.notificationsTitle}</h2>
							{#if told && told.lines.length > 0}
								<a class="more" href={notificationsHref()}>All</a>
							{/if}
						</header>
						{#if toldProblem}
							<p class="quiet" role="alert">{toldProblem}</p>
							<button class="button-plain again" type="button" onclick={() => (askedTold += 1)}>
								{app.said?.common.tryAgain ?? 'Try again'}
							</button>
						{:else if !told}
							<p class="quiet" role="status">Loading…</p>
						{:else if told.lines.length === 0}
							<p class="quiet">{told.emptyLine}</p>
						{:else}
							<ul class="group-card" aria-label={said.notificationsTitle}>
								{#each told.lines.slice(0, TOLD_LINES) as line (`${line.atUnixS}:${line.handle}:${line.flightId ?? ''}:${line.commentId ?? ''}:${line.about}`)}
									<li>
										<a
											class="group-row told"
											href={line.flightId
												? publicFlightHref(line.flightId)
												: pilotHref(line.handle)}
										>
											<span class="line">
												<span>{line.line}</span>
												<span class="age numeric">{line.age}</span>
											</span>
											<span class="chevron"><Icon name="forward" /></span>
										</a>
									</li>
								{/each}
							</ul>
						{/if}
					</section>
				{/if}
			</div>

			<!-- The pilots who fly near the pilot, for everyone. -->
			<section class="part span-12" aria-labelledby="{id}-near">
				<header><h2 id="{id}-near">{app.said?.picture.nearTitle ?? 'Pilots near'}</h2></header>
				<PilotsNear labelledby="{id}-near" place={near.place} />
			</section>
		</div>
	{/if}
</div>

{#if groups && signedIn}
	<PlanForm open={adding} {groups} onclose={() => (adding = false)} />
{/if}
{#if openPlans && signedIn}
	<OpenPlanForm
		open={addingOpen}
		model={openPlans}
		near={near.place}
		onclose={() => (addingOpen = false)}
	/>
{/if}

<style>
	.quiet {
		color: var(--text-secondary);
	}

	.part {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		min-width: 0;
	}

	.part > header {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: var(--space-3);
	}

	h2 {
		font-size: var(--title);
		line-height: 1.3;
	}

	.more {
		flex: none;
		padding: 0;
		background: none;
		color: var(--link);
		font-size: var(--subheadline);
		font-weight: 500;
	}

	button.more {
		min-height: var(--target);
		font-weight: 600;
	}

	@media (hover: hover) {
		.more:hover {
			text-decoration: underline;
		}
	}

	.note {
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	.again {
		align-self: flex-start;
		margin-left: calc(-1 * var(--space-3));
	}

	/* Cards side by side as far as the width has room, one under the
	   other on a phone. */
	.cards {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(min(100%, 15rem), 1fr));
		gap: var(--space-3);
		list-style: none;
	}

	.feed {
		grid-template-columns: repeat(auto-fill, minmax(min(100%, 17rem), 1fr));
	}

	/* A few words and one thing to do about them. */
	.words {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: var(--space-2);
		padding: var(--space-4);
	}

	.words .button-plain {
		margin-left: calc(-1 * var(--space-3));
		box-shadow: none;
	}

	.sign-in .button-prominent {
		box-shadow: none;
	}

	.side {
		display: flex;
		flex-direction: column;
		gap: var(--space-5);
		min-width: 0;
	}

	/* The pack's field on the page, the found pilots in a card under it. */
	.result {
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	.result:not(:empty) {
		padding: var(--space-2) var(--space-1) 0;
	}

	.find ul {
		margin-top: var(--space-2);
		list-style: none;
	}

	.group-row {
		align-items: center;
		gap: var(--space-3);
	}

	.line {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		min-width: 0;
		overflow-wrap: anywhere;
	}

	.age {
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	.chevron {
		flex: none;
		color: var(--icon-quiet);
		font-size: 0.75rem;
	}

	/* Narrow: a round button with a plus, as in the iPhone's toolbar. */
	.new {
		display: grid;
		place-items: center;
		width: var(--target);
		height: var(--target);
		border-radius: 50%;
		background: var(--surface);
		color: var(--link);
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

	.community > .page-head {
		flex-wrap: nowrap;
	}

	/* Wide: the button has room to say what it does. */
	@media (min-width: 56rem) {
		.new {
			display: inline-flex;
			gap: 0.375rem;
			width: auto;
			padding: 0 1.125rem 0 0.875rem;
			border-radius: var(--radius-pill);
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
