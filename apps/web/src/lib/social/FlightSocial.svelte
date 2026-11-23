<!--
	Cheers and comments under a public flight: the core's line of who
	cheered with one button, then the comments, the oldest first, and for a
	signed-in pilot a field to write one in. Signed out, the lines alone.

	Every word is the core's (`cheerList`, `commentList`, `commentDraft`,
	`socialWords`), and so is every refusal. A comment is another pilot's
	plain text: it is set as text with its line breaks, never as markup, and
	no link is made of it.
-->
<script lang="ts">
	import { getApp } from '#lib/app/app.svelte.ts';
	import FaceStack from '#lib/community/FaceStack.svelte';
	import PilotFace from '#lib/community/PilotFace.svelte';
	import ReportSheet from '#lib/community/ReportSheet.svelte';
	import ConfirmDialog from '#lib/design/ConfirmDialog.svelte';
	import type { CheerList, CommentDraft, CommentList, CommentRow } from '#lib/core/index.ts';

	interface Props {
		/** The flight's address on the server. */
		flightId: string;
		/** The handle of the flight's pilot: they may remove what is written under it. */
		flightHandle: string;
		/** Told the two lines whenever they change: "3 pilots cheered", "2 comments". */
		onlines?: (cheers: string, comments: string) => void;
	}

	let { flightId, flightHandle, onlines }: Props = $props();

	const app = getApp();
	const social = $derived(app.social);
	const said = $derived(social?.words);
	const signedIn = $derived(Boolean(app.account?.signedIn));
	/** Nobody cheers a flight of their own: the server refuses it. */
	const own = $derived(app.account?.account?.handle === flightHandle);
	const id = $props.id();

	let cheers = $state.raw<CheerList>();
	let comments = $state.raw<CommentList>();
	/** Why the cheers or the comments are missing, or a change was refused, in words. */
	let problem = $state.raw<string>();
	let working = $state(false);

	let text = $state('');
	let draft = $state.raw<CommentDraft>();
	/** The comment being reported, while its form is open. */
	let reported = $state.raw<CommentRow>();
	let reporting = $state(false);
	/** The comment the pilot is asked about before it is removed. */
	let removing = $state.raw<CommentRow>();
	let asking = $state(false);

	/** How many pages of comments are read at once: a flight has few. */
	const PAGES_AT_MOST = 5;

	const told = (error: unknown) => (error instanceof Error ? error.message : String(error));

	async function read(core: NonNullable<typeof social>['core'], current: () => boolean) {
		try {
			const answer = await core.cheers(flightId);
			if (current()) cheers = answer;
		} catch (error) {
			if (current()) problem = told(error);
		}
		try {
			let list = await core.comments(flightId, flightHandle);
			for (let page = 1; list.next && page < PAGES_AT_MOST; page += 1) {
				const more = await core.comments(flightId, flightHandle, list.next);
				list = { ...more, comments: [...list.comments, ...more.comments] };
			}
			if (current()) comments = list;
		} catch (error) {
			if (current()) problem = told(error);
		}
	}

	// Asked again when the pilot signs in or out: the answers then say what
	// is their own.
	$effect(() => {
		const core = social?.core;
		void signedIn;
		if (!core) return;
		let current = true;
		problem = undefined;
		void read(core, () => current);
		return () => {
			current = false;
		};
	});

	$effect(() => {
		onlines?.(cheers?.line ?? '', comments?.countLine ?? '');
	});

	// The core looks at the comment as it is written.
	$effect(() => {
		const core = social?.core;
		const typed = text;
		if (!core) return;
		let current = true;
		void core.commentDraft(typed).then(
			(looked) => {
				if (current) draft = looked;
			},
			() => {}
		);
		return () => {
			current = false;
		};
	});

	async function cheer() {
		if (!social || !cheers || working) return;
		working = true;
		problem = undefined;
		try {
			const state = await social.core.cheer(flightId, !cheers.cheered);
			cheers = {
				...cheers,
				cheered: state.cheered,
				button: state.button,
				count: state.cheers,
				line: state.line
			};
			// Who cheered is read again, so that the pilot's own face comes or goes.
			cheers = await social.core.cheers(flightId);
		} catch (error) {
			problem = told(error);
		} finally {
			working = false;
		}
	}

	async function send(event: SubmitEvent) {
		event.preventDefault();
		if (!social || !comments || !draft?.canSend || working) return;
		working = true;
		problem = undefined;
		try {
			const posted = await social.core.comment(flightId, draft.text);
			comments = {
				...comments,
				count: posted.count,
				countLine: posted.countLine,
				comments: [...comments.comments, { ...posted.comment, canRemove: true }]
			};
			text = '';
		} catch (error) {
			problem = told(error);
		} finally {
			working = false;
		}
	}

	async function remove(comment: CommentRow) {
		if (!social || working) return;
		working = true;
		problem = undefined;
		try {
			await social.core.removeComment(flightId, comment.id);
			// The count's words are the core's: the list is read again.
			comments = await social.core.comments(flightId, flightHandle);
		} catch (error) {
			problem = told(error);
		} finally {
			working = false;
		}
	}
</script>

{#if said && (cheers || comments)}
	<section class="social" aria-label={said.commentsTitle}>
		{#if cheers && (cheers.line || (signedIn && !own))}
			<div class="cheers">
				{#if signedIn && !own}
					<button
						class={cheers.cheered ? 'button-plain' : 'button-prominent'}
						type="button"
						aria-pressed={cheers.cheered ?? false}
						disabled={working}
						onclick={cheer}
					>
						{cheers.button}
					</button>
				{/if}
				<FaceStack pilots={cheers.pilots} />
				<p class="line numeric" role="status">{cheers.line}</p>
			</div>
		{/if}

		{#if comments}
			<h3 id="{id}-title">{comments.countLine || said.commentsTitle}</h3>
			{#if comments.comments.length > 0}
				<ul aria-labelledby="{id}-title">
					{#each comments.comments as comment (comment.id)}
						<li>
							<p class="byline">
								<PilotFace picture={comment.picturePath} name={comment.name} />
								<span>{comment.byline}</span>
							</p>
							<p class="text">{comment.text}</p>
							{#if signedIn}
								<p class="acts">
									{#if comment.canRemove}
										<button
											type="button"
											disabled={working}
											onclick={() => {
												removing = comment;
												asking = true;
											}}
										>
											{said.commentRemove}
										</button>
									{/if}
									{#if comment.handle !== app.account?.account?.handle}
										<button
											type="button"
											onclick={() => {
												reported = comment;
												reporting = true;
											}}
										>
											{said.commentReport}
										</button>
									{/if}
								</p>
							{/if}
						</li>
					{/each}
				</ul>
			{/if}

			{#if comments.closedLine}
				<p class="quiet">{comments.closedLine}</p>
			{:else if signedIn}
				<form onsubmit={send}>
					<label class="visually-hidden" for="{id}-write">{said.commentPlaceholder}</label>
					<textarea
						id="{id}-write"
						rows="2"
						placeholder={said.commentPlaceholder}
						aria-describedby="{id}-notice"
						bind:value={text}
						oninput={() => (problem = undefined)}></textarea>
					<div class="send">
						{#if draft && text !== ''}
							<span class="counter numeric" class:over={Boolean(draft.fault)}>{draft.counter}</span>
						{/if}
						<button class="button-plain" type="submit" disabled={!draft?.canSend || working}>
							{said.commentSend}
						</button>
					</div>
					{#if draft?.fault && text !== ''}
						<p class="refusal" role="status">{draft.fault}</p>
					{/if}
					{#if draft}
						<p class="quiet" id="{id}-notice">{draft.notice}</p>
					{/if}
				</form>
			{/if}
		{/if}

		{#if problem}
			<p class="refusal" role="alert">{problem}</p>
		{/if}
	</section>

	<ReportSheet
		open={reporting}
		comment={reported ? { flightId, commentId: reported.id } : undefined}
		title={said.commentReportTitle}
		onclose={() => (reporting = false)}
	/>
	<!-- The core's question before a comment goes; whose it is and how old says which. -->
	<ConfirmDialog
		open={asking}
		title={said.commentRemoveQuestion}
		message={removing?.byline ?? ''}
		confirm={said.commentRemove}
		cancel={app.said?.common.cancel}
		onconfirm={() => {
			asking = false;
			if (removing) void remove(removing);
		}}
		oncancel={() => (asking = false)}
	/>
{/if}

<style>
	.social {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}

	.cheers {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2) var(--space-3);
	}

	.line {
		color: var(--text-secondary);
		font-size: var(--subheadline);
	}

	h3 {
		font-size: var(--subheadline);
		font-weight: 600;
	}

	ul {
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}

	.byline {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		font-size: var(--footnote);
		color: var(--text-secondary);
		overflow-wrap: anywhere;
	}

	.byline span {
		min-width: 0;
	}

	/* The author's face before the byline, as small as a row's. */
	.byline :global(.face) {
		width: 1.5rem;
		height: 1.5rem;
	}

	/* Another pilot's plain text, with its line breaks. */
	.text {
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}

	.acts {
		display: flex;
		gap: var(--space-4);
	}

	.acts button {
		min-height: var(--target);
		color: var(--link);
		font-size: var(--footnote);
	}

	form {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	textarea {
		width: 100%;
		padding: var(--space-2) var(--space-3);
		border: 0;
		border-radius: var(--radius-control);
		background: var(--fill);
		color: var(--text);
		font: inherit;
		resize: vertical;
	}

	.send {
		display: flex;
		align-items: center;
		justify-content: flex-end;
		gap: var(--space-3);
	}

	.counter,
	.quiet {
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	.counter.over,
	.refusal {
		font-size: var(--footnote);
		color: var(--danger);
	}
</style>
