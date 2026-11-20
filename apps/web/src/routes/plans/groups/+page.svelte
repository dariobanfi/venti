<!--
	The pilot's groups, and the two ways into one: making it, which shows its
	invite code once, or joining with the code its keeper hands out. The
	iPhone app's `GroupsList` is its twin.

	A group is seen by its members and nobody else, so there is nothing here
	to search.
-->
<script lang="ts">
	import { getApp } from '#lib/app/app.svelte.ts';
	import { groupHref, plansHref } from '#lib/app/routes.ts';
	import Icon from '#lib/design/Icon.svelte';
	import PromptDialog from '#lib/design/PromptDialog.svelte';
	import InviteCodeSheet from '#lib/groups/InviteCodeSheet.svelte';
	import WithAccount from '#lib/groups/WithAccount.svelte';
	import { WHAT_A_GROUP_IS } from '#lib/groups/words.ts';

	const app = getApp();
	const groups = $derived(app.groups);

	$effect(() => groups?.keepCurrent());

	/** Which of the two dialogs is open, if one is. */
	let asking = $state<'make' | 'join'>();
	let working = $state(false);
	/** Why what was typed was not taken, in words. */
	let refusal = $state.raw<string>();
	/** What became of joining, in the core's line. */
	let outcome = $state.raw<string>();
	/** A group just made, to show its invite code once. */
	let made = $state.raw<{ group: string; code: string }>();

	function ask(about: 'make' | 'join') {
		refusal = undefined;
		outcome = undefined;
		asking = about;
	}

	async function act(work: () => Promise<void>) {
		if (!groups || working) return;
		working = true;
		refusal = undefined;
		try {
			await work();
			asking = undefined;
		} catch (error) {
			// The dialog stays, with the reason under its field.
			refusal = error instanceof Error ? error.message : String(error);
		} finally {
			working = false;
		}
	}

	const make = (name: string) =>
		act(async () => {
			const group = await groups!.make(name);
			made = { group: group.group.name, code: group.inviteCode };
		});

	const join = (code: string) =>
		act(async () => {
			outcome = (await groups!.join(code)).words;
		});
</script>

<svelte:head>
	<title>Groups · Venti</title>
</svelte:head>

<div class="settings-page">
	<header>
		<a class="back-link" href={plansHref()}>
			<Icon name="back" />
			Flying plans
		</a>
	</header>
	<h1>Groups</h1>

	<WithAccount>
		{#snippet children(groups)}
			{#if groups.board}
				{#if groups.groups.length > 0}
					<section class="group" aria-label="Your groups">
						<ul class="group-card">
							{#each groups.groups as group (group.id)}
								<li>
									<a class="group-row" href={groupHref(group.id)}>
										<span class="pair">
											<span>{group.name}</span>
											<span class="lesser">{group.summary}</span>
										</span>
										<span class="chevron"><Icon name="forward" /></span>
									</a>
								</li>
							{/each}
						</ul>
					</section>
				{/if}

				<section class="group" aria-label="A new group">
					<div class="group-card">
						<button class="group-row" type="button" onclick={() => ask('make')}>
							Make a group
							<Icon name="plus" />
						</button>
						<button class="group-row" type="button" onclick={() => ask('join')}>
							Join with a code
							<Icon name="key" />
						</button>
					</div>
					{#if outcome}
						<p class="group-note outcome" role="status">
							<Icon name="check" />
							{outcome}
						</p>
					{/if}
					<p class="group-note">{WHAT_A_GROUP_IS}</p>
				</section>
			{:else if groups.problem}
				<p class="lead" role="alert">{groups.problem}</p>
			{:else}
				<p class="lead" role="status">Asking the server…</p>
			{/if}
		{/snippet}
	</WithAccount>
</div>

<PromptDialog
	open={asking === 'make'}
	title="Make a group"
	message="You will keep it: you hand out its invite code and can remove a member."
	label="Its name"
	confirm="Make"
	{working}
	{refusal}
	onconfirm={make}
	oncancel={() => (asking = undefined)}
/>

<PromptDialog
	open={asking === 'join'}
	title="Join a group"
	message="The group’s keeper has the code."
	label="Invite code"
	confirm="Join"
	verbatim
	{working}
	{refusal}
	onconfirm={join}
	oncancel={() => (asking = undefined)}
/>

<InviteCodeSheet shown={made} onclose={() => (made = undefined)} />

<style>
	.lead {
		margin-top: 0.5rem;
		padding: 0 1rem;
		color: var(--text-secondary);
	}

	.pair {
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
		min-width: 0;
		padding: 0.125rem 0;
	}

	.pair > span {
		overflow-wrap: anywhere;
	}

	.lesser {
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	.chevron {
		color: var(--icon-quiet);
		font-size: 0.75rem;
	}

	.group-row :global(svg) {
		font-size: 0.875em;
	}

	.chevron :global(svg) {
		font-size: 1em;
	}

	.outcome {
		display: flex;
		align-items: flex-start;
		gap: 0.375rem;
		color: var(--text);
	}

	.outcome :global(svg) {
		flex: none;
		color: var(--link);
		font-size: 1em;
	}
</style>
