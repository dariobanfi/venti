<!--
	The history of a site that pilots made: its revisions, the newest first,
	each in the core's line ("Changed by Mara Beispiel (@mara) on 21 Nov
	2026: “fixed the height”") with what it changed under it, field by field.

	A signed-in pilot can undo the last change, where the core names the
	revision that does it, and put the site back to any revision the core
	says can be put back: that is also how a removed site comes back. Both
	ask for the line that is kept with the new revision first ("Why", which
	may stay empty) and are sent as the core's request; a refusal stands in
	the core's words.
-->
<script lang="ts">
	import type { SiteHistory, SiteWritten } from '#lib/core/index.ts';
	import PromptDialog from '#lib/design/PromptDialog.svelte';
	import { said, type SiteEditsModel } from './edit.svelte';

	interface Props {
		edits: SiteEditsModel;
		/** The site, by its identifier. */
		id: string;
		/** Whether the pilot can change the site: signed in, on a server that takes sites. */
		canWrite: boolean;
		/** Called with the site as it stands after a revision was put back. */
		onwritten: (written: SiteWritten) => void;
		/** Reports one revision to whoever runs the server. */
		onreport?: (revision: number) => void;
	}

	let { edits, id, canWrite, onwritten, onreport }: Props = $props();

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

	/** The revision to put back, with the words on what does it, while its "Why" is asked. */
	let putting = $state.raw<{ revision: number; title: string }>();
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
			<button class="button-plain" type="button" onclick={() => load(id)}>Try again</button>
		</div>
	{:else}
		{@const history = phase.history}
		{#if outcome}
			<p class="group-card words" role="status">{outcome}</p>
		{/if}
		{#if canWrite && history.undoRevision !== undefined}
			{@const undo = history.undoRevision}
			<div class="group-card">
				<button
					class="group-row"
					type="button"
					onclick={() => (putting = { revision: undo, title: history.undoWords })}
				>
					{history.undoWords}
				</button>
			</div>
		{/if}
		<ol class="group-card" aria-label="Revisions, the newest first">
			{#each history.revisions.toReversed() as revision (revision.number)}
				<li>
					<p>{revision.words}</p>
					{#each revision.changes as change (change)}
						<p class="lesser">{change}</p>
					{/each}
					{#if (canWrite && revision.canBePutBack) || onreport}
						<div class="acts">
							{#if canWrite && revision.canBePutBack}
								<button
									class="button-plain"
									type="button"
									onclick={() =>
										(putting = { revision: revision.number, title: 'Put back to this' })}
								>
									Put back to this
								</button>
							{/if}
							{#if onreport}
								<button
									class="button-plain"
									type="button"
									onclick={() => onreport(revision.number)}
								>
									Report
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
	title={putting?.title ?? ''}
	label={edits.form?.fields.find((field) => field.field === 'comment')?.title ?? ''}
	message={edits.form?.fields.find((field) => field.field === 'comment')?.help}
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

	.acts {
		display: flex;
		gap: var(--space-2);
		margin: 0.25rem 0 -0.5rem -0.75rem;
	}

	.group-note a {
		color: var(--link);
	}
</style>
