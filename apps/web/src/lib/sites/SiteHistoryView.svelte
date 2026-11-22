<!--
	The history of a site that pilots made: its revisions, the newest first,
	each in the core's line ("Changed by Mara Beispiel (@mara) on 21 Nov
	2026: “fixed the height”") with what it changed under it, field by field.

	What may be done in it is the core's one answer for the history as it
	was read (`Core.siteHistoryOffers`), so a history opened by the site's
	identifier alone is answered too: to undo the last change, where the
	history names the revision that does it, to put the site back to any
	revision the core says can be put back, to bring a removed site back
	("Bring this site back": the revision before the removal), and to
	report a revision. Its note, where something is held back, stands over
	the revisions. Every one of these words is the core's. Each asks for
	the line that is kept with the new revision first ("Why", which may
	stay empty) and is sent as the core's request; a refusal stands in the
	core's words.
-->
<script lang="ts">
	import { getApp } from '#lib/app/app.svelte.ts';
	import type { SiteHistory, SiteHistoryOffers, SiteWritten } from '#lib/core/index.ts';
	import PromptDialog from '#lib/design/PromptDialog.svelte';
	import { said, type SiteEditsModel } from './edit.svelte';

	const app = getApp();

	interface Props {
		edits: SiteEditsModel;
		/** The site, by its identifier. */
		id: string;
		/** Called with the site as it stands after a revision was put back. */
		onwritten: (written: SiteWritten) => void;
		/** Reports one revision to whoever runs the server. */
		onreport?: (revision: number) => void;
	}

	let { edits, id, onwritten, onreport }: Props = $props();

	const words = $derived(edits.words);

	type Phase =
		{ is: 'loading' } | { is: 'ready'; history: SiteHistory } | { is: 'failed'; why: string };

	let phase = $state.raw<Phase>({ is: 'loading' });
	/** Counts the requests, so that a slow answer for another site is dropped. */
	let asked = 0;

	function load(site: string) {
		const mine = ++asked;
		phase = { is: 'loading' };
		edits.history(site).then(
			(history) => {
				if (mine === asked) phase = { is: 'ready', history };
			},
			(error) => {
				if (mine === asked) phase = { is: 'failed', why: said(error) };
			}
		);
	}

	$effect(() => load(id));

	/**
	 * What the core offers this pilot in the history as it was read; asked
	 * again when the account changes. Nothing is offered until it has said.
	 */
	let offers = $state.raw<SiteHistoryOffers>();
	$effect(() => {
		void edits.asked;
		const read = phase.is === 'ready' ? phase.history : undefined;
		if (!read) {
			offers = undefined;
			return;
		}
		let current = true;
		edits.historyOffers(read).then(
			(answer) => {
				if (current) offers = answer;
			},
			() => {}
		);
		return () => {
			current = false;
		};
	});

	/**
	 * The revision to put back, with the words on what does it and what
	 * that means, while its "Why" is asked.
	 */
	let putting = $state.raw<{
		revision: number;
		/** The words on the button that does it. */
		title: string;
		/** The core's question, where it has one for this. */
		question?: string;
		about?: string;
	}>();
	let working = $state(false);
	let refusal = $state.raw<string>();
	/** The core's line about the last revision put back. */
	let outcome = $state.raw<string>();

	async function putBack(comment: string) {
		if (!putting || working) return;
		working = true;
		refusal = undefined;
		try {
			const written = await edits.restore(id, putting.revision, comment);
			putting = undefined;
			outcome = written.words;
			onwritten(written);
			load(id);
		} catch (error) {
			refusal = said(error);
		} finally {
			working = false;
		}
	}
</script>

<div class="history">
	{#if phase.is === 'loading'}
		<p class="waiting" role="status">Loading…</p>
	{:else if phase.is === 'failed'}
		<div class="group-card words">
			<p role="alert">{phase.why}</p>
			<button class="button-plain" type="button" onclick={() => load(id)}
				>{app.said?.common.tryAgain}</button
			>
		</div>
	{:else}
		{@const history = phase.history}
		{#if outcome}
			<p class="group-card words" role="status">{outcome}</p>
		{/if}
		{#if offers?.note}
			<p class="group-note held">{offers.note}</p>
		{/if}
		{#if offers?.bringBack.isOffered && history.bringBackRevision !== undefined}
			{@const back = history.bringBackRevision}
			{@const title = offers.bringBack.words}
			<div class="group-card">
				<button
					class="group-row"
					type="button"
					onclick={() =>
						(putting = {
							revision: back,
							title,
							question: words?.bringBackQuestion,
							about: words?.bringBackAbout
						})}
				>
					{title}
				</button>
			</div>
		{/if}
		{#if offers?.undo.isOffered && history.undoRevision !== undefined}
			{@const undo = history.undoRevision}
			{@const title = offers.undo.words}
			<div class="group-card">
				<button
					class="group-row"
					type="button"
					onclick={() => (putting = { revision: undo, title })}
				>
					{title}
				</button>
			</div>
		{/if}
		<ol class="group-card" aria-label="Revisions, the newest first">
			{#each history.revisions.toReversed() as revision (revision.number)}
				{@const putBack = Boolean(offers?.putBack.isOffered) && revision.canBePutBack}
				{@const report = onreport && offers?.reportRevision.isOffered ? onreport : undefined}
				<li>
					<p>{revision.words}</p>
					{#each revision.changes as change (change)}
						<p class="lesser">{change}</p>
					{/each}
					{#if putBack || report}
						<div class="acts">
							{#if putBack}
								{@const title = revision.putBackWords || (offers?.putBack.words ?? '')}
								<button
									class="button-plain"
									type="button"
									onclick={() =>
										(putting = {
											revision: revision.number,
											title,
											question: history.isRemoved
												? words?.bringBackQuestion
												: words?.putBackQuestion,
											about: history.isRemoved ? words?.bringBackAbout : words?.putBackAbout
										})}
								>
									{title}
								</button>
							{/if}
							{#if report}
								<button class="button-plain" type="button" onclick={() => report(revision.number)}>
									{offers?.reportRevision.words}
								</button>
							{/if}
						</div>
					{/if}
				</li>
			{/each}
		</ol>
		{#if history.credit}
			<p class="group-note">
				<a href={history.credit.url} target="_blank" rel="noopener noreferrer">
					{history.credit.text}
				</a>
			</p>
		{/if}
	{/if}
</div>

<!-- The line kept with the new revision, asked before anything is sent. -->
<PromptDialog
	open={putting !== undefined}
	title={putting?.question || (putting?.title ?? '')}
	label={edits.form?.fields.find((field) => field.field === 'comment')?.title ?? ''}
	message={[putting?.about, edits.form?.fields.find((field) => field.field === 'comment')?.help]
		.filter(Boolean)
		.join(' ')}
	confirm={putting?.title ?? ''}
	optional
	{working}
	{refusal}
	onconfirm={putBack}
	oncancel={() => {
		putting = undefined;
		refusal = undefined;
	}}
/>

<style>
	.history {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}

	.waiting {
		padding: 1rem;
		color: var(--text-secondary);
		text-align: center;
	}

	.words {
		padding: 0.75rem 1rem;
	}

	/* The core's note on what is held back: over the revisions, close to them. */
	.held {
		padding-top: 0;
	}

	.words .button-plain {
		margin: 0.25rem 0 -0.5rem -0.75rem;
		/* Not a row of the card: no hairline over it. */
		box-shadow: none;
	}

	li {
		padding: 0.75rem 1rem;
		overflow-wrap: anywhere;
	}

	.lesser {
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	/* The core's words on these are long: they go under each other where
	   they do not fit side by side. */
	.acts {
		display: flex;
		flex-wrap: wrap;
		gap: 0 var(--space-2);
		margin: 0.25rem 0 -0.5rem -0.75rem;
	}

	.group-note a {
		color: var(--link);
	}
</style>
