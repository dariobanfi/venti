<!--
	One group with its chat (owner decision 26), at `/community/groups/<id>/`.
	On a wide screen the chat stands beside the group's picture, place,
	description and members; on a phone the chat takes the width under a
	short head. The newest messages are at the bottom; scrolling up reads
	the page before; what is new is asked for every five seconds while the
	page is in front (`chat.svelte.ts`). A member writes text, adds up to
	four pictures and Venti links, which the server shows as cards. A
	public group is read by every pilot signed in and joined with one
	button; a private one answers "not found" to all but its members and
	invited pilots. "New plan" posts a plan into the group, where it shows
	as a post with "I am in".
-->
<script lang="ts">
	import { untrack } from 'svelte';
	import { page } from '$app/state';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { accountHref, groupsHref } from '#lib/app/routes.ts';
	import { pictureToSend, PICTURE_KINDS } from '#lib/account/picture.ts';
	import PilotFace from '#lib/community/PilotFace.svelte';
	import ReportSheet from '#lib/community/ReportSheet.svelte';
	import { reportedGroup } from '#lib/community/report.ts';
	import type { ChatMessage, CommunityDetail } from '#lib/core/index.ts';
	import Icon from '#lib/design/Icon.svelte';
	import { PICTURES_AT_MOST } from '#lib/groups/chat.ts';
	import { ChatModel } from '#lib/groups/chat.svelte.ts';
	import ChatMessageView from '#lib/groups/ChatMessageView.svelte';
	import GroupSettings from '#lib/groups/GroupSettings.svelte';
	import PlanForm from '#lib/groups/PlanForm.svelte';
	import { memberName } from '#lib/groups/words.ts';

	const app = getApp();
	const communities = $derived(app.communities);
	const signedIn = $derived(app.account?.signedIn ?? false);
	const myName = $derived(app.account?.account?.displayName);
	const groupId = $derived(decodeURIComponent(page.params.id ?? ''));
	const said = $derived(communities?.words);

	let detail = $state.raw<CommunityDetail>();
	let problem = $state<string>();
	let asked = 0;

	async function load() {
		if (!communities || !signedIn || !groupId) return;
		const asking = ++asked;
		try {
			const found = await communities.core.community(groupId);
			if (asking !== asked) return;
			detail = found;
			problem = undefined;
		} catch (error) {
			if (asking === asked) problem = error instanceof Error ? error.message : String(error);
		}
	}

	$effect(() => {
		void groupId;
		void signedIn;
		detail = undefined;
		void load();
	});

	/** The chat, once the pilot may read it. */
	const canRead = $derived(detail?.canRead ?? false);
	let chat = $state.raw<ChatModel>();
	$effect(() => {
		if (!communities || !canRead) {
			chat = undefined;
			return;
		}
		const model = new ChatModel(communities.core, groupId, () => {
			void app.unread?.read();
			communities.changed();
		});
		chat = model;
		return model.open();
	});

	// Joining a public group, or an invited one.
	let joining = $state(false);
	async function join() {
		if (!communities || joining) return;
		joining = true;
		try {
			await communities.join(groupId);
			await load();
		} catch (error) {
			problem = error instanceof Error ? error.message : String(error);
		} finally {
			joining = false;
		}
	}

	// The list scrolls on its own: the newest at the bottom, the page before when scrolled to the top.
	let list = $state<HTMLElement>();
	let top = $state<HTMLElement>();
	let atBottom = true;
	let shownCount = 0;
	/** How far the list's end was from where it was scrolled to, before the messages changed. */
	let fromTheEnd: number | undefined;
	$effect.pre(() => {
		void chat?.messages;
		const box = untrack(() => list);
		fromTheEnd = box ? box.scrollHeight - box.scrollTop : undefined;
	});
	$effect(() => {
		const messages = chat?.messages ?? [];
		const box = list;
		if (!box) return;
		const grew = messages.length > shownCount;
		shownCount = messages.length;
		if (!grew) return;
		// At the bottom it follows what is new; a page from before keeps what was in view.
		if (atBottom || fromTheEnd === undefined) box.scrollTop = box.scrollHeight;
		else box.scrollTop = box.scrollHeight - fromTheEnd;
	});
	function scrolled() {
		if (!list) return;
		atBottom = list.scrollHeight - list.scrollTop - list.clientHeight < 48;
	}
	$effect(() => {
		const sentinel = top;
		const model = chat;
		if (!sentinel || !model || !list) return;
		const seen = new IntersectionObserver(
			(entries) => {
				if (entries.some((entry) => entry.isIntersecting) && model.loaded) {
					atBottom = false;
					void model.loadOlder();
				}
			},
			{ root: list }
		);
		seen.observe(sentinel);
		return () => seen.disconnect();
	});

	// Writing a message.
	let draft = $state('');
	let files = $state.raw<File[]>([]);
	let sending = $state(false);
	let sendProblem = $state<string>();

	function choose(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		const chosen = [...(input.files ?? [])];
		input.value = '';
		files = [...files, ...chosen].slice(0, PICTURES_AT_MOST);
	}

	async function send(event: SubmitEvent) {
		event.preventDefault();
		if (!chat || sending) return;
		if (draft.trim() === '' && files.length === 0) return;
		sending = true;
		sendProblem = undefined;
		try {
			const pictures: string[] = [];
			for (const file of files) {
				const { bytes, mediaType } = await pictureToSend(file);
				pictures.push((await chat.sendPicture(bytes, mediaType)).id);
			}
			await chat.post({ text: draft, pictures, links: [] });
			draft = '';
			files = [];
			atBottom = true;
		} catch (error) {
			sendProblem = error instanceof Error ? error.message : String(error);
		} finally {
			sending = false;
		}
	}

	function keydown(event: KeyboardEvent) {
		// Enter sends, Shift and Enter makes a new line, as in every chat.
		if (event.key === 'Enter' && !event.shiftKey && !event.isComposing) {
			event.preventDefault();
			(event.currentTarget as HTMLTextAreaElement).form?.requestSubmit();
		}
	}

	async function remove(message: ChatMessage) {
		try {
			await chat?.remove(message);
		} catch (error) {
			sendProblem = error instanceof Error ? error.message : String(error);
		}
	}

	let reporting = $state.raw<{ group: string; id: string; picture: boolean }>();
	function report(message: ChatMessage, picture?: number) {
		reporting =
			picture === undefined
				? { group: groupId, id: message.id, picture: false }
				: { group: groupId, id: message.pictureIds[picture], picture: true };
	}

	let settings = $state(false);
	/** Whether the form that reports the group itself is open. */
	let reportingGroup = $state(false);
	let planning = $state(false);

	const name = $derived(detail?.group.name ?? 'Group');
	const id = $props.id();
</script>

<svelte:head>
	<title>{name} · Venti</title>
</svelte:head>

<div class="desk group-page">
	<header class="page-head">
		<a class="back-link" href={groupsHref()}>
			<Icon name="back" />
			Community
		</a>
	</header>

	{#if app.failure}
		<p class="quiet">Venti could not open its data. {app.failure}</p>
	{:else if !app.account}
		<p class="quiet" role="status">Opening…</p>
	{:else if !signedIn}
		<div class="group-card words">
			<p>A group and its chat are for pilots with an account on Venti.</p>
			<a class="button-plain" href={accountHref()}>Sign in</a>
		</div>
	{:else if !detail}
		{#if problem}
			<p class="quiet" role="alert">{problem}</p>
		{:else}
			<p class="quiet" role="status">Loading…</p>
		{/if}
	{:else}
		{@const group = detail.group}
		<div class="layout">
			<aside class="about" aria-labelledby="{id}-name">
				<div class="head">
					<PilotFace picture={group.picturePath} name={group.name} size="l" />
					<div class="titles">
						<h1 id="{id}-name">{group.name}</h1>
						<p class="summary">{group.kindWords} · {group.summary}</p>
					</div>
				</div>
				{#if group.description}
					<p class="description">{group.description}</p>
				{/if}
				<div class="actions">
					{#if detail.canJoin}
						<button class="button-prominent" type="button" disabled={joining} onclick={join}>
							{said?.join ?? 'Join'}
						</button>
					{/if}
					{#if detail.canPost}
						<button class="button-plain" type="button" onclick={() => (planning = true)}>
							<Icon name="calendar" /> New plan
						</button>
					{/if}
					{#if group.myRole !== 'none' && group.myRole !== 'invited'}
						<button class="button-plain" type="button" onclick={() => (settings = true)}>
							<Icon name="settings" /> Settings
						</button>
					{/if}
					{#if group.myRole !== 'owner'}
						<button class="button-plain" type="button" onclick={() => (reportingGroup = true)}>
							<Icon name="warning" /> Report this group
						</button>
					{/if}
				</div>
				<section class="members" aria-labelledby="{id}-members">
					<h2 class="group-title" id="{id}-members">Members</h2>
					<ul class="group-card" aria-labelledby="{id}-members">
						{#each detail.members as member (member.handle)}
							<li class="member">
								<PilotFace name={member.name} size="xs" />
								<span class="who">{memberName(member)}</span>
								{#if member.roleWords}<span class="group-value">{member.roleWords}</span>{/if}
							</li>
						{/each}
					</ul>
				</section>
			</aside>

			<section class="chat" aria-label="Chat">
				{#if !canRead}
					<p class="quiet">Join the group to read its chat.</p>
				{:else if chat}
					<div class="list" bind:this={list} onscroll={scrolled}>
						<div class="top" bind:this={top}>
							{#if chat.loadingOlder}
								<p class="quiet" role="status">Loading…</p>
							{:else if chat.hasOlder}
								<button class="button-plain" type="button" onclick={() => chat?.loadOlder()}>
									Earlier messages
								</button>
							{/if}
						</div>
						{#if !chat.loaded}
							<p class="quiet" role="status">{chat.problem ?? 'Loading…'}</p>
						{:else if chat.messages.length === 0}
							<p class="quiet empty">{said?.emptyChat ?? 'No messages yet.'}</p>
						{:else}
							<ul class="messages" aria-label="Messages">
								{#each chat.messages as message (message.id)}
									<ChatMessageView
										{message}
										canModerate={detail.canModerate}
										groups={app.groups}
										{myName}
										imInWord={said?.imIn}
										onremove={() => remove(message)}
										onreport={(picture) => report(message, picture)}
										onchanged={() => chat?.refresh()}
									/>
								{/each}
							</ul>
						{/if}
					</div>
					{#if chat.loaded && chat.problem}
						<p class="note" role="status">{chat.problem}</p>
					{/if}
					{#if detail.canPost}
						<form class="compose" onsubmit={send}>
							{#if files.length > 0}
								<ul class="chosen" aria-label="Pictures to send">
									{#each files as file, index (`${file.name}:${index}`)}
										<li>
											<span class="file-name">{file.name}</span>
											<button
												type="button"
												class="off"
												aria-label="Leave out {file.name}"
												onclick={() => (files = files.filter((_, at) => at !== index))}
											>
												<Icon name="close" />
											</button>
										</li>
									{/each}
								</ul>
							{/if}
							<div class="row">
								<label class="add" title="Add pictures">
									<Icon name="plus" />
									<span class="visually-hidden">Add pictures</span>
									<input
										type="file"
										accept={PICTURE_KINDS}
										multiple
										disabled={files.length >= PICTURES_AT_MOST}
										onchange={choose}
									/>
								</label>
								<textarea
									rows="1"
									aria-label={said?.messagePlaceholder ?? 'Message'}
									placeholder={said?.messagePlaceholder ?? 'Message'}
									bind:value={draft}
									onkeydown={keydown}
									oninput={() => (sendProblem = undefined)}></textarea>
								<button
									class="send"
									type="submit"
									disabled={sending || (draft.trim() === '' && files.length === 0)}
								>
									{said?.send ?? 'Send'}
								</button>
							</div>
							{#if sendProblem}
								<p class="note refusal" role="alert">{sendProblem}</p>
							{/if}
						</form>
					{/if}
				{/if}
			</section>
		</div>

		<GroupSettings
			open={settings}
			{detail}
			model={communities!}
			onchanged={(changed) => (detail = changed)}
			onclose={() => (settings = false)}
		/>
		{#if app.groups}
			<PlanForm
				open={planning}
				groups={app.groups}
				inGroup={groupId}
				onsaved={() => chat?.refresh()}
				onclose={() => (planning = false)}
			/>
		{/if}
		<ReportSheet
			open={reportingGroup}
			subject={reportedGroup(groupId).subject}
			title={reportedGroup(groupId).title}
			onclose={() => (reportingGroup = false)}
		/>
		<ReportSheet
			open={reporting !== undefined}
			message={reporting}
			title={reporting?.picture ? 'Report this picture' : 'Report this message'}
			onclose={() => (reporting = undefined)}
		/>
	{/if}
</div>

<style>
	.quiet {
		color: var(--text-secondary);
	}

	.words {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: var(--space-2);
		padding: var(--space-4);
	}

	.layout {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		min-width: 0;
	}

	.about {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		min-width: 0;
	}

	.head {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		min-width: 0;
	}

	.titles {
		min-width: 0;
	}

	h1 {
		overflow-wrap: anywhere;
	}

	.summary {
		font-size: var(--subheadline);
		color: var(--text-secondary);
	}

	.description {
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}

	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
	}

	.actions .button-plain {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
	}

	/* On a phone the members are in the settings: the chat comes first. */
	.members {
		display: none;
	}

	.member {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-2) var(--space-4);
	}

	.who {
		flex: 1;
		min-width: 0;
		overflow-wrap: anywhere;
	}

	.chat {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		min-width: 0;
	}

	.list {
		height: min(65dvh, 40rem);
		overflow-y: auto;
		padding: var(--space-2) var(--space-3);
		background: var(--surface);
		border: 1px solid var(--separator);
		border-radius: var(--radius-card);
		overscroll-behavior: contain;
	}

	.top {
		display: flex;
		justify-content: center;
		min-height: 1px;
	}

	.messages {
		list-style: none;
	}

	.empty {
		padding: var(--space-5) 0;
		text-align: center;
	}

	.note {
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	.refusal {
		color: var(--danger);
	}

	.compose {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.row {
		display: flex;
		align-items: flex-end;
		gap: var(--space-2);
	}

	.add {
		position: relative;
		flex: none;
		display: grid;
		place-items: center;
		width: var(--target);
		height: var(--target);
		border-radius: 50%;
		background: var(--fill);
		color: var(--link);
		cursor: pointer;
	}

	.add input {
		position: absolute;
		inset: 0;
		opacity: 0;
		cursor: pointer;
	}

	textarea {
		flex: 1;
		min-width: 0;
		min-height: var(--target);
		max-height: 10rem;
		padding: 0.75rem 1rem;
		border: 1px solid var(--separator);
		border-radius: var(--radius-control);
		background: var(--surface);
		color: var(--text);
		font: inherit;
		resize: vertical;
	}

	textarea::placeholder {
		color: var(--text-secondary);
	}

	.send {
		flex: none;
		min-height: var(--target);
		padding: 0 var(--space-4);
		border-radius: var(--radius-pill);
		background: var(--accent);
		color: var(--on-accent);
		font-weight: 600;
	}

	.send:disabled {
		opacity: 0.5;
	}

	.chosen {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
		list-style: none;
	}

	.chosen li {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		padding-left: var(--space-3);
		border-radius: var(--radius-pill);
		background: var(--fill);
		font-size: var(--footnote);
	}

	.file-name {
		max-width: 12rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.off {
		display: grid;
		place-items: center;
		width: var(--target);
		height: var(--target);
		border-radius: 50%;
		color: var(--text-secondary);
		font-size: 0.75rem;
	}

	/* Wide: the chat beside the group. */
	@media (min-width: 56rem) {
		.layout {
			display: grid;
			grid-template-columns: minmax(16rem, 1fr) minmax(0, 2fr);
			align-items: start;
		}

		.members {
			display: flex;
			flex-direction: column;
			gap: var(--space-2);
		}

		.members ul {
			list-style: none;
		}

		.list {
			height: min(70dvh, 46rem);
		}
	}
</style>
