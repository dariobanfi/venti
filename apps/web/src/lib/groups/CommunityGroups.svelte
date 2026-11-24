<!--
	Community's groups (owner decision 26): the pilot's invites into
	private groups, to accept or decline; the pilot's own groups as cards,
	with the last message and what is unread; and the public groups near
	the pilot (their position, else the place the home page's weather is
	for, else the region's middle: `near-place.svelte.ts`), or those a
	search by name or place finds, each to join. "New group" opens the form.

	Finding groups needs an account, as every part of a group does
	(docs/api.md): signed out, Community says so in its sign-in card.
-->
<script lang="ts">
	import { goto } from '$app/navigation';
	import { groupHref } from '#lib/app/routes.ts';
	import type { CommunityInvite, Coordinate, FoundCommunity } from '#lib/core/index.ts';
	import type { CommunitiesModel } from './communities.svelte';
	import CommunityForm from './CommunityForm.svelte';
	import GroupCard from './GroupCard.svelte';

	interface Props {
		model: CommunitiesModel;
		/** The place the groups near are for. */
		near: Coordinate;
	}

	let { model, near }: Props = $props();

	const id = $props.id();
	const said = $derived(model.words);

	$effect(() => model.keepCurrent());

	/** What the search asks for; empty for the groups near. */
	let text = $state('');
	let found = $state.raw<FoundCommunity[]>();
	let finding = $state(false);
	let findProblem = $state<string>();
	/** Counts the searches, so that an older answer never replaces a newer. */
	let asked = 0;

	// The groups near, or those the text finds; a moment after the pilot stops typing.
	$effect(() => {
		const typed = text.trim();
		const place = near;
		const asking = ++asked;
		finding = true;
		const wait = setTimeout(
			() => {
				model.find(typed ? undefined : place, typed).then(
					(answer) => {
						if (asking !== asked) return;
						found = answer;
						findProblem = undefined;
						finding = false;
					},
					(error) => {
						if (asking !== asked) return;
						findProblem = error instanceof Error ? error.message : String(error);
						finding = false;
					}
				);
			},
			typed ? 300 : 0
		);
		return () => clearTimeout(wait);
	});

	/** After joining, the card says so: the search is asked again. */
	async function join(group: FoundCommunity) {
		await model.join(group.group.id);
		await goto(groupHref(group.group.id));
	}

	let inviteProblem = $state<string>();
	async function answer(invite: CommunityInvite, accept: boolean) {
		inviteProblem = undefined;
		try {
			if (accept) {
				await model.join(invite.group.id);
				await goto(groupHref(invite.group.id));
			} else {
				await model.decline(invite);
			}
		} catch (error) {
			inviteProblem = error instanceof Error ? error.message : String(error);
		}
	}

	let making = $state(false);
</script>

<section class="part span-12" id="groups" aria-labelledby="{id}-groups">
	<header>
		<h2 id="{id}-groups">{said?.title ?? 'Groups'}</h2>
		<button class="more" type="button" onclick={() => (making = true)}>New group</button>
	</header>

	{#if model.invites.length > 0}
		<ul class="group-card invites" aria-label="Invites">
			{#each model.invites as invite (invite.group.id)}
				<li class="invite">
					<span class="words">{invite.words}</span>
					<span class="buttons">
						<button class="button-plain" type="button" onclick={() => answer(invite, true)}>
							{said?.join ?? 'Join'}
						</button>
						<button class="button-plain" type="button" onclick={() => answer(invite, false)}>
							Decline
						</button>
					</span>
				</li>
			{/each}
		</ul>
		{#if inviteProblem}
			<p class="quiet" role="alert">{inviteProblem}</p>
		{/if}
	{/if}

	{#if model.mine}
		{#if model.mine.length === 0}
			<p class="quiet">You are in no group yet. Join one near you, or make one.</p>
		{:else}
			<ul class="cards" aria-label={said?.yours ?? 'Your groups'}>
				{#each model.mine as group (group.id)}
					<GroupCard {group} />
				{/each}
			</ul>
		{/if}
	{:else if model.problem}
		<p class="quiet" role="alert">{model.problem}</p>
		<button class="button-plain again" type="button" onclick={() => model.load()}>Try again</button>
	{:else}
		<p class="quiet" role="status">Loading…</p>
	{/if}
</section>

<section class="part span-12" aria-labelledby="{id}-near">
	<header>
		<h2 id="{id}-near">{text.trim() ? 'Found' : (said?.nearYou ?? 'Near you')}</h2>
	</header>
	<input
		class="text-field search"
		type="search"
		aria-label={said?.search ?? 'Search by name or place'}
		placeholder={said?.search ?? 'Search by name or place'}
		autocomplete="off"
		bind:value={text}
	/>
	{#if findProblem}
		<p class="quiet" role="alert">{findProblem}</p>
	{:else if found && found.length === 0 && !finding}
		<p class="quiet">{said?.noneFound ?? 'No group here yet.'}</p>
	{:else if found}
		<ul class="cards" aria-label={text.trim() ? 'Found' : (said?.nearYou ?? 'Near you')}>
			{#each found as one (one.group.id)}
				<GroupCard
					group={one.group}
					distance={one.distance}
					joinWord={said?.join}
					onjoin={one.isMember ? undefined : () => join(one)}
				/>
			{/each}
		</ul>
	{:else}
		<p class="quiet" role="status">Loading…</p>
	{/if}
</section>

<CommunityForm open={making} {model} onclose={() => (making = false)} />

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
		min-height: var(--target);
		padding: 0;
		background: none;
		color: var(--link);
		font-size: var(--subheadline);
		font-weight: 600;
	}

	.cards {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(min(100%, 17rem), 1fr));
		gap: var(--space-3);
		list-style: none;
	}

	.invites {
		list-style: none;
	}

	.invite {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-2);
		padding: var(--space-2) var(--space-4);
	}

	.invite .words {
		min-width: 0;
		overflow-wrap: anywhere;
	}

	.buttons {
		display: flex;
		gap: var(--space-1);
	}

	.search {
		max-width: 30rem;
	}

	.again {
		align-self: flex-start;
		margin-left: calc(-1 * var(--space-3));
	}
</style>
