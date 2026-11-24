<!--
	"About the group", in the phones' order and words: its members, then
	for its owner and admins inviting, changing and the picture, then muting
	and leaving. For its members (owner decision 26): muting it;
	for its owner and admins, changing its name, description and home and
	its picture, inviting a pilot by handle and removing members who are no
	admins; for its owner alone, making members admins, switching the group
	public or private, handing it on and deleting it. Leaving is everyone's.
	What would end something is asked about first. Who may do what is the
	server's rule and the core's word (`canModerate`, each member's role).
-->
<script lang="ts">
	import { messageOf } from '#lib/core/index.ts';
	import { goto } from '$app/navigation';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { groupsHref } from '#lib/app/routes.ts';
	import { pictureToSend, PICTURE_KINDS } from '#lib/account/picture.ts';
	import type { CommunityDetail, CommunityDraft, CommunityMember } from '#lib/core/index.ts';
	import ConfirmDialog from '#lib/design/ConfirmDialog.svelte';
	import Menu, { type MenuItem } from '#lib/design/Menu.svelte';
	import Sheet from '#lib/design/Sheet.svelte';
	import PilotFace from '#lib/community/PilotFace.svelte';
	import type { CommunitiesModel } from './communities.svelte';
	import CommunityForm from './CommunityForm.svelte';
	import { type Asking, memberName, question } from './words';

	interface Props {
		open: boolean;
		detail: CommunityDetail;
		model: CommunitiesModel;
		/** Told the group as it now is. */
		onchanged: (detail: CommunityDetail) => void;
		onclose: () => void;
	}

	let { open, detail, model, onchanged, onclose }: Props = $props();

	const app = getApp();

	const id = $props.id();
	const core = $derived(model.core);
	const group = $derived(detail.group);
	const isOwner = $derived(group.myRole === 'owner');
	const said = $derived(app.said?.community);

	let problem = $state<string>();
	let working = $state(false);
	let handle = $state('');
	let invited = $state<string>();
	let editing = $state(false);

	let asked = $state.raw<Asking>({ about: 'delete' });
	let confirming = $state(false);
	const asking = $derived(question(asked, said));

	const said_ = (error: unknown) => messageOf(error);

	/** Does one thing; afterwards the group is asked for as it now is. */
	async function act(work: () => Promise<unknown>, thenLeave = false) {
		if (working) return;
		working = true;
		problem = undefined;
		try {
			await work();
			if (thenLeave) {
				model.changed();
				onclose();
				await goto(groupsHref());
				return;
			}
			onchanged(await core.community(group.id));
			model.changed();
		} catch (error) {
			problem = said_(error);
		} finally {
			working = false;
		}
	}

	function ask(about: Asking) {
		asked = about;
		confirming = true;
	}

	function confirmed() {
		confirming = false;
		const about = asked;
		switch (about.about) {
			case 'leave':
				return act(() => core.leaveGroup(group.id), true);
			case 'delete':
				return act(() => core.deleteGroup(group.id), true);
			case 'remove':
				return act(() => core.removeGroupMember(group.id, about.member.handle));
			case 'handOn':
				return act(() => core.handOverGroup(group.id, about.member.handle));
		}
	}

	async function invite(event: SubmitEvent) {
		event.preventDefault();
		if (working) return;
		working = true;
		problem = undefined;
		invited = undefined;
		try {
			invited = await core.inviteToCommunity(group.id, handle);
			handle = '';
		} catch (error) {
			problem = said_(error);
		} finally {
			working = false;
		}
	}

	async function mute(muted: boolean) {
		await act(() => core.muteCommunity(group.id, muted));
	}

	async function picture(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		const file = input.files?.[0];
		input.value = '';
		if (!file) return;
		await act(async () => {
			const { bytes, mediaType } = await pictureToSend(file);
			return core.setCommunityPicture(group.id, bytes, mediaType);
		});
	}

	async function change(draft: CommunityDraft) {
		const changed = await core.changeCommunity(group.id, draft);
		onchanged(changed);
		model.changed();
	}

	/** What the owner and the admins can do about another member. */
	function memberActions(member: CommunityMember): MenuItem[] {
		const items: MenuItem[] = [];
		if (isOwner) {
			items.push(
				member.role === 'admin'
					? {
							label: 'Make a member again',
							icon: 'person',
							onselect: () => act(() => core.setCommunityAdmin(group.id, member.handle, false))
						}
					: {
							label: 'Make admin',
							icon: 'key',
							onselect: () => act(() => core.setCommunityAdmin(group.id, member.handle, true))
						},
				{
					label: `Make ${member.name} the owner`,
					icon: 'forward',
					onselect: () => ask({ about: 'handOn', member })
				}
			);
		}
		if (isOwner || member.role === 'member') {
			items.push({
				label: 'Remove from the group',
				icon: 'trash',
				destructive: true,
				onselect: () => ask({ about: 'remove', member })
			});
		}
		return items;
	}
</script>

<Sheet {open} title="About the group" {onclose}>
	<div class="settings">
		<section class="group first" aria-labelledby="{id}-members">
			<h3 class="group-title" id="{id}-members">Members</h3>
			<ul class="group-card" aria-labelledby="{id}-members">
				{#each detail.members as member (member.handle)}
					{@const actions = detail.canModerate && !member.isMe ? memberActions(member) : []}
					<li class="member">
						<PilotFace name={member.name} />
						<span class="pair">
							<span>{memberName(member)}</span>
							<span class="lesser">@{member.handle}</span>
						</span>
						{#if member.roleWords}
							<span class="group-value">{member.roleWords}</span>
						{/if}
						{#if actions.length > 0}
							<Menu label="More about {member.name}" items={actions} />
						{/if}
					</li>
				{/each}
			</ul>
		</section>

		{#if detail.canModerate}
			<section class="group">
				<h3 class="group-title" id="{id}-invite">Invite by handle</h3>
				<form class="invite" onsubmit={invite}>
					<input
						class="text-field"
						aria-labelledby="{id}-invite"
						placeholder="@handle"
						autocomplete="off"
						bind:value={handle}
					/>
					<button class="button-plain" type="submit" disabled={working || handle.trim() === ''}>
						{said?.invite}
					</button>
				</form>
				<p class="group-note" role={invited ? 'status' : undefined}>
					{invited ?? 'By the handle on their pilot’s page.'}
				</p>
			</section>

			<section class="group">
				<div class="group-card">
					<button class="group-row" type="button" onclick={() => (editing = true)}>
						Change name, description and home
					</button>
					<label class="group-row file">
						<span>{group.picturePath ? 'Change the picture' : 'Give it a picture'}</span>
						<input type="file" accept={PICTURE_KINDS} onchange={picture} />
					</label>
					{#if group.picturePath}
						<button
							class="group-row destructive"
							type="button"
							onclick={() => act(() => core.removeCommunityPicture(group.id))}
						>
							Remove the picture
						</button>
					{/if}
					{#if isOwner}
						<label class="group-row toggle">
							<span>{said?.public}</span>
							<input
								type="checkbox"
								role="switch"
								checked={group.isPublic}
								disabled={working}
								onchange={(event) => {
									const on = event.currentTarget.checked;
									void act(() => core.setCommunityPublic(group.id, on));
								}}
							/>
						</label>
					{/if}
				</div>
				<p class="group-note">
					{group.isPublic
						? 'Anyone signed in finds, reads and joins a public group.'
						: 'Only members read a private group; you and its admins invite pilots into it.'}
				</p>
			</section>
		{/if}

		<section class="group">
			<div class="group-card">
				<label class="group-row toggle">
					<span>{said?.mute}</span>
					<input
						type="checkbox"
						role="switch"
						checked={group.muted}
						disabled={working}
						onchange={(event) => mute(event.currentTarget.checked)}
					/>
				</label>
			</div>
			<p class="group-note">
				{group.isPublic
					? 'A public group’s messages reach your phone only where they mention you.'
					: 'Each new message reaches your phone unless the group is muted.'}
			</p>
		</section>

		{#if problem}
			<p class="group-note refusal" role="alert">{problem}</p>
		{/if}

		<section class="group">
			<div class="group-card">
				<button
					class="group-row destructive"
					type="button"
					onclick={() => ask({ about: 'leave', asOwner: isOwner, isPublic: group.isPublic })}
				>
					{said?.leave}
				</button>
				{#if isOwner}
					<button
						class="group-row destructive"
						type="button"
						onclick={() => ask({ about: 'delete' })}
					>
						Delete the group
					</button>
				{/if}
			</div>
			{#if isOwner}
				<p class="group-note">Choose a member to hand the group on to them before you leave.</p>
			{/if}
		</section>
	</div>
</Sheet>

<ConfirmDialog
	open={confirming}
	title={asking.title}
	message={asking.message}
	confirm={asking.confirm}
	destructive
	onconfirm={confirmed}
	oncancel={() => (confirming = false)}
/>

<CommunityForm
	open={editing}
	{model}
	editing={group}
	onchange={change}
	onclose={() => (editing = false)}
/>

<style>
	.settings {
		display: flex;
		flex-direction: column;
	}

	.first {
		margin-top: 0.25rem;
	}

	.invite {
		display: flex;
		gap: var(--space-2);
		align-items: center;
	}

	.invite input {
		flex: 1;
		min-width: 0;
	}

	.member {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		padding: var(--space-2) var(--space-4);
	}

	.member .pair {
		flex: 1;
		display: flex;
		flex-direction: column;
		min-width: 0;
	}

	.lesser {
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	.file {
		position: relative;
		color: var(--link);
		cursor: pointer;
	}

	.file input {
		position: absolute;
		inset: 0;
		opacity: 0;
		cursor: pointer;
	}

	.refusal {
		color: var(--danger);
	}
</style>
