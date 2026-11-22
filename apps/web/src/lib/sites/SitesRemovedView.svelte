<!--
	"Removed lately": the sites of Venti's two tiers that are removed, the
	one removed last first, each in the core's line ("Musterkogel: removed
	by Jonas Beispiel (@jonas) on 21 Nov 2026"). The partners' stand first;
	the places from pilots stand under their layer's name, and only while
	their switch is on.

	The lines are the core's (`Core.sitesRemoved`): one answer of the server
	names the removed sites of a tier, and each line says whether whoever is
	signed in here may bring its site back, with the words of that entry.
	Bringing one back reads that site's history, which names the revision
	that does it (`bringBackRevision`), asks with the core's question and
	for the line kept with the new revision ("Why", which may stay empty),
	and is sent as the core's request; a refusal stands in the core's words.
-->
<script lang="ts">
	import { getApp } from '#lib/app/app.svelte.ts';
	import type { SiteRemovedLine, SiteWritten } from '#lib/core/index.ts';
	import PromptDialog from '#lib/design/PromptDialog.svelte';
	import { said, type SiteEditsModel } from './edit.svelte';

	const app = getApp();

	interface Props {
		edits: SiteEditsModel;
		/** Whether the places from pilots are switched on: their removed ones are listed then. */
		withPilots: boolean;
		/** The name of the layer of the places from pilots, the core's: "Places from pilots". */
		pilotsTitle: string;
		/** Called with the site as it stands after it was brought back, and the core's line. */
		onwritten: (written: SiteWritten) => void;
	}

	let { edits, withPilots, pilotsTitle, onwritten }: Props = $props();

	const id = $props.id();
	const words = $derived(edits.words);
	const why = $derived(edits.form?.fields.find((field) => field.field === 'comment'));

	type Phase =
		| { is: 'loading' }
		| { is: 'ready'; partners: SiteRemovedLine[]; pilots: SiteRemovedLine[] }
		| { is: 'failed'; why: string };

	let phase = $state.raw<Phase>({ is: 'loading' });
	/** Counts the requests, so that an answer overtaken by a later one is dropped. */
	let asked = 0;

	function load(pilotsToo: boolean) {
		const mine = ++asked;
		phase = { is: 'loading' };
		Promise.all([edits.removed(false), pilotsToo ? edits.removed(true) : []]).then(
			([partners, pilots]) => {
				if (mine === asked) phase = { is: 'ready', partners, pilots };
			},
			(error) => {
				if (mine === asked) phase = { is: 'failed', why: said(error) };
			}
		);
	}

	// Asked again when the account changes: who may bring a site back follows it.
	$effect(() => {
		void edits.asked;
		load(withPilots);
	});

	/** The site to bring back and the revision that does it, while its "Why" is asked. */
	let bringing = $state.raw<{ line: SiteRemovedLine; revision: number }>();
	/** The site whose history is being read for that revision. */
	let reading = $state.raw<string>();
	/** Why a site could not be brought back, at its line. */
	let problem = $state.raw<{ id: string; why: string }>();
	let working = $state(false);
	let refusal = $state.raw<string>();

	/** "Bring this site back": the site's history names the revision that does it. */
	async function bringBack(line: SiteRemovedLine) {
		if (reading) return;
		reading = line.id;
		problem = undefined;
		try {
			const history = await edits.history(line.id);
			if (history.bringBackRevision === undefined) {
				// It stands again: someone else was quicker. The list says so when read anew.
				load(withPilots);
				return;
			}
			bringing = { line, revision: history.bringBackRevision };
		} catch (error) {
			problem = { id: line.id, why: said(error) };
		} finally {
			reading = undefined;
		}
	}

	async function send(comment: string) {
		if (!bringing || working) return;
		working = true;
		refusal = undefined;
		try {
			const written = await edits.restore(bringing.line.id, bringing.revision, comment);
			bringing = undefined;
			onwritten(written);
		} catch (error) {
			refusal = said(error);
		} finally {
			working = false;
		}
	}
</script>

{#snippet lines(
	removed: SiteRemovedLine[],
	label: string | undefined,
	labelledBy: string | undefined
)}
	<ol class="group-card" aria-label={label} aria-labelledby={labelledBy}>
		{#each removed as line (line.id)}
			<li>
				<p>{line.words}</p>
				{#if problem?.id === line.id}
					<p class="refusal" role="alert">{problem.why}</p>
				{/if}
				{#if line.mayBringBack}
					<div class="acts">
						<button
							class="button-plain"
							type="button"
							disabled={reading !== undefined}
							onclick={() => void bringBack(line)}
						>
							{line.bringBackWords}
						</button>
					</div>
				{/if}
			</li>
		{/each}
	</ol>
{/snippet}

<div class="removed">
	{#if phase.is === 'loading'}
		<p class="waiting" role="status">Loading…</p>
	{:else if phase.is === 'failed'}
		<div class="group-card words">
			<p role="alert">{phase.why}</p>
			<button class="button-plain" type="button" onclick={() => load(withPilots)}>
				{app.said?.common.tryAgain}
			</button>
		</div>
	{:else}
		{#if phase.partners.length === 0 && phase.pilots.length === 0}
			<p class="waiting" role="status">No site was removed.</p>
		{/if}
		{#if phase.partners.length > 0}
			{@render lines(phase.partners, words?.removedTitle, undefined)}
		{/if}
		{#if phase.pilots.length > 0}
			<section class="tier">
				<h3 class="group-title" id="{id}-pilots">{pilotsTitle}</h3>
				{@render lines(phase.pilots, undefined, `${id}-pilots`)}
			</section>
		{/if}
	{/if}
</div>

<!-- The core's question, and the line kept with the new revision, before anything is sent. -->
<PromptDialog
	open={bringing !== undefined}
	title={words?.bringBackQuestion ?? ''}
	label={why?.title ?? ''}
	message={[words?.bringBackAbout, why?.help].filter(Boolean).join(' ')}
	confirm={bringing?.line.bringBackWords ?? ''}
	optional
	{working}
	{refusal}
	onconfirm={send}
	oncancel={() => {
		bringing = undefined;
		refusal = undefined;
	}}
/>

<style>
	.removed {
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

	ol {
		list-style: none;
	}

	li {
		padding: 0.75rem 1rem;
		overflow-wrap: anywhere;
	}

	.refusal {
		font-size: var(--footnote);
		color: var(--danger);
	}

	/* The entry under its line, drawn out to the line's edge as a plain button is. */
	.acts {
		display: flex;
		flex-wrap: wrap;
		margin: 0.25rem 0 -0.5rem -0.75rem;
	}
</style>
