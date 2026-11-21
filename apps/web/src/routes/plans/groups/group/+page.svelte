<!--
	One group: its members, and what its keeper or a member can do. The
	keeper makes a new invite code, renames the group, removes a member or
	hands the keeping on, and deletes the group; anyone leaves it. Each of
	these is asked about first, with what it means. Which group is in the
	address: `?id=`. The iPhone app's `GroupScreen` is its twin.

	Who may do what is the server's rule and the core's word: the screen
	offers the keeper's part where the core says the pilot keeps the group,
	and shows a refusal in the core's words.
-->
<script lang="ts">
	import { browser } from '$app/env';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { groupsHref } from '#lib/app/routes.ts';
	import { reportedGroup } from '#lib/community/report.ts';
	import ReportSheet from '#lib/community/ReportSheet.svelte';
	import type { GroupDetail, GroupMember } from '#lib/core/index.ts';
	import ConfirmDialog from '#lib/design/ConfirmDialog.svelte';
	import Icon from '#lib/design/Icon.svelte';
	import Menu from '#lib/design/Menu.svelte';
	import PromptDialog from '#lib/design/PromptDialog.svelte';
	import InviteCodeSheet from '#lib/groups/InviteCodeSheet.svelte';
	import WithAccount from '#lib/groups/WithAccount.svelte';
	import { type Asking, memberName, question } from '#lib/groups/words.ts';

	const app = getApp();
	const groups = $derived(app.groups);
	const signedIn = $derived(app.account?.signedIn ?? false);

	/** The group asked for. The address is not known while the pages are written out at build time. */
	const id = $derived(browser ? page.url.searchParams.get('id') : null);

	/** The group with its members, once the server has answered. */
	let detail = $state.raw<GroupDetail>();
	/** Why the group is not there, or why the last thing did not work, in words. */
	let problem = $state.raw<string>();
	let working = $state(false);
	/** What is asked about before it is done. The dialog keeps its words while it closes. */
	let asked = $state.raw<Asking>({ about: 'newCode' });
	let confirming = $state(false);
	let renaming = $state(false);
	/** Whether the form that reports the group to whoever runs the server is open. */
	let reporting = $state(false);
	/** Why the new name was not taken, in words. */
	let refusal = $state.raw<string>();
	/** A new invite code, to show once. */
	let code = $state.raw<{ group: string; code: string }>();

	/** What the group is called, also before the server has said who is in it. */
	const name = $derived(
		detail?.group.name ?? groups?.groups.find((group) => group.id === id)?.name ?? 'Group'
	);

	// The group is asked for when the screen opens, and again for whoever
	// signs in.
	$effect(() => {
		if (!groups || !signedIn) return;
		if (!id) {
			problem = 'This address names no group.';
			return;
		}
		// Only the answer to the latest question counts.
		let current = true;
		detail = undefined;
		problem = undefined;
		groups.detail(id).then(
			(found) => {
				if (current) detail = found;
			},
			(error) => {
				if (current) problem = said(error);
			}
		);
		return () => {
			current = false;
		};
	});

	const said = (error: unknown) => (error instanceof Error ? error.message : String(error));

	/**
	 * Does one thing on the server. Afterwards the pilot is back among the
	 * groups, or sees the group as it now is; a refusal is shown in words.
	 */
	async function act(work: (id: string) => Promise<GroupDetail | void>, thenLeave = false) {
		if (!groups || !id || working) return;
		working = true;
		problem = undefined;
		try {
			const changed = await work(id);
			if (thenLeave) await goto(groupsHref());
			else detail = changed ?? (await groups.detail(id));
		} catch (error) {
			problem = said(error);
		} finally {
			working = false;
		}
	}

	function ask(about: Asking) {
		asked = about;
		confirming = true;
	}

	/** Goes ahead with what was asked about. */
	function confirmed() {
		confirming = false;
		const model = groups!;
		const about = asked;
		switch (about.about) {
			case 'newCode':
				return act(async (id) => {
					code = { group: name, code: await model.newInvite(id) };
					return detail;
				});
			case 'leave':
				return act((id) => model.leave(id), true);
			case 'delete':
				return act((id) => model.delete(id), true);
			case 'remove':
				return act((id) => model.remove(id, about.member.handle));
			case 'handOver':
				return act((id) => model.handOver(id, about.member.handle));
			default:
				return undefined;
		}
	}

	async function rename(typed: string) {
		if (!groups || !id || working) return;
		working = true;
		refusal = undefined;
		try {
			detail = await groups.rename(id, typed);
			renaming = false;
		} catch (error) {
			// The dialog stays, with the reason under its field.
			refusal = said(error);
		} finally {
			working = false;
		}
	}

	/** What the keeper can do about another member. */
	const memberActions = (member: GroupMember) => [
		{
			label: 'Make keeper',
			icon: 'key' as const,
			onselect: () => ask({ about: 'handOver', member })
		},
		{
			label: 'Remove from the group',
			icon: 'trash' as const,
			destructive: true,
			onselect: () => ask({ about: 'remove', member })
		}
	];
</script>

<svelte:head>
	<title>{name} · Venti</title>
</svelte:head>

<div class="settings-page">
	<header>
		<a class="back-link" href={groupsHref()}>
			<Icon name="back" />
			Groups
		</a>
	</header>
	<h1>{name}</h1>

	<WithAccount>
		{#snippet children()}
			{#if detail}
				{@const kept = detail.group.isKeptByMe}
				<section class="group" aria-labelledby="group-members">
					<h2 class="group-title" id="group-members">Members</h2>
					<ul class="group-card" aria-labelledby="group-members">
						{#each detail.members as member (member.handle)}
							<li class="member">
								<span class="pair">
									<span>{memberName(member)}</span>
									<span class="lesser">@{member.handle}</span>
								</span>
								{#if member.isKeeper}
									<span class="group-value">Keeper</span>
								{/if}
								{#if kept && !member.isMe}
									<Menu label="More about {member.name}" items={memberActions(member)} />
								{/if}
							</li>
						{/each}
					</ul>
				</section>

				{#if kept}
					<section class="group" aria-label="Keeping the group">
						<div class="group-card">
							<button
								class="group-row"
								type="button"
								disabled={working}
								onclick={() => ask({ about: 'newCode' })}
							>
								New invite code
								<Icon name="key" />
							</button>
							<button
								class="group-row"
								type="button"
								disabled={working}
								onclick={() => {
									refusal = undefined;
									renaming = true;
								}}
							>
								Rename the group
							</button>
						</div>
						<p class="group-note">
							You keep this group. The menu beside a member removes them or hands the keeping on.
							Removing a member ends the invite code, which they know.
						</p>
					</section>
				{/if}

				<section class="group" aria-label="Leaving">
					<div class="group-card">
						<button
							class="group-row destructive"
							type="button"
							disabled={working}
							onclick={() => ask({ about: 'leave', asKeeper: kept })}
						>
							Leave the group
						</button>
						{#if kept}
							<button
								class="group-row destructive"
								type="button"
								disabled={working}
								onclick={() => ask({ about: 'delete' })}
							>
								Delete the group
							</button>
						{/if}
						<!-- A group someone else keeps can be reported to whoever runs the server. -->
						{#if !kept}
							<button
								class="group-row"
								type="button"
								disabled={working}
								onclick={() => (reporting = true)}
							>
								Report this group
							</button>
						{/if}
					</div>
					<p class="group-note">
						{#if kept}
							When you leave, the member who has been in longest keeps the group. Your plans go with
							you.
						{:else}
							Your plans, and your word on the plans of others, go with you.
						{/if}
					</p>
				</section>
			{:else if !problem}
				<p class="lead" role="status">Asking Venti…</p>
			{/if}
			{#if problem}
				<p class={detail ? 'group-note refusal' : 'lead'} role="alert">{problem}</p>
			{/if}
		{/snippet}
	</WithAccount>
	{#if id}
		<ReportSheet open={reporting} {...reportedGroup(id)} onclose={() => (reporting = false)} />
	{/if}
</div>

<ConfirmDialog
	open={confirming}
	{...question(asked)}
	onconfirm={confirmed}
	oncancel={() => (confirming = false)}
/>

<PromptDialog
	open={renaming}
	title="Rename the group"
	message="Every member sees the new name."
	label="Its name"
	value={detail?.group.name ?? ''}
	confirm="Rename"
	{working}
	{refusal}
	onconfirm={rename}
	oncancel={() => (renaming = false)}
/>

<InviteCodeSheet shown={code} onclose={() => (code = undefined)} />

<style>
	.lead {
		margin-top: 0.5rem;
		padding: 0 1rem;
		color: var(--text-secondary);
	}

	.member {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		min-height: var(--target);
		padding: 0.375rem 0.5rem 0.375rem 1rem;
	}

	.pair {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
		min-width: 0;
	}

	.pair > span {
		overflow-wrap: anywhere;
	}

	.lesser {
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	.group-value {
		color: var(--text-secondary);
		font-size: var(--footnote);
		font-weight: 500;
		/* In line with the rows that have no menu at their end. */
		padding-right: 0.5rem;
	}

	.group-row :global(svg) {
		font-size: 0.875em;
	}

	.refusal {
		color: var(--danger);
	}
</style>
