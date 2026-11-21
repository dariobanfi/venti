<!--
	The form for a flying site a pilot adds to Venti's own layer, or corrects
	there. Everything it says is the core's (`siteForm`): the fields in the
	core's order, each with its title and its line of help, the uses and the
	sixteen wind directions as chips to tick, the answers about guests'
	rules, what giving a site means with the licence as a link, and the words
	on the button. What is wrong with a draft is the core's too, shown at its
	field once the pilot has tried to send; the button stays there to be
	pressed, so that the pilot learns what is missing.

	The place is shown in words, with "Move": the form then gives way to the
	map, where the pilot moves the map under a mark (`onmove`).

	The fields are the pack's inputs, as the plans' form has them.
-->
<script lang="ts">
	import type { SiteForm, SiteFormField, SiteKind, SiteWritten } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import type { SiteDraft } from './edit.svelte';

	interface Props {
		form: SiteForm;
		/** The site being written, which outlives this form. */
		editing: SiteDraft;
		/** Whether the chips of the wind scroll sideways in one row, on a narrow screen. */
		compact?: boolean;
		/** The pilot wants to move the place on the map. */
		onmove: () => void;
		/** Called with the site as the server wrote it, and the core's line about it. */
		onsent: (written: SiteWritten) => void;
		/** Opens another site, by its identifier: the one that stands in the way. */
		onopen: (id: string) => void;
	}

	let { form, editing, compact = false, onmove, onsent, onopen }: Props = $props();

	const id = $props.id();
	const text = format();
	const draft = $derived(editing.draft);

	/** The height as the pilot types it; empty for a height that is not known. */
	let height = $state('');
	// Filled when the form opens on a draft, and not again while the pilot types.
	let filledFor: unknown;
	$effect.pre(() => {
		if (filledFor === editing) return;
		filledFor = editing;
		height = editing.draft.elevationM === undefined ? '' : String(editing.draft.elevationM);
	});

	function typedHeight(typed: string) {
		height = typed;
		const metres = typed.trim() === '' ? undefined : Number(typed);
		editing.set({ elevationM: metres === undefined || Number.isNaN(metres) ? undefined : metres });
	}

	function toggleUse(kind: SiteKind) {
		const uses = draft.uses.includes(kind)
			? draft.uses.filter((one) => one !== kind)
			: [...draft.uses, kind];
		editing.set({ uses });
	}

	function toggleWind(fromDeg: number) {
		const windFromDeg = draft.windFromDeg.includes(fromDeg)
			? draft.windFromDeg.filter((one) => one !== fromDeg)
			: [...draft.windFromDeg, fromDeg];
		editing.set({ windFromDeg });
	}

	/** The sentence about giving, around the licence's name, which is a link. */
	const giving = $derived.by(() => {
		const at = form.giving.indexOf(form.licence);
		return at < 0
			? { before: `${form.giving} `, after: '' }
			: { before: form.giving.slice(0, at), after: form.giving.slice(at + form.licence.length) };
	});

	async function send(event: SubmitEvent) {
		event.preventDefault();
		const written = await editing.send();
		if (written) onsent(written);
	}

	const typed = (event: Event) => (event.currentTarget as HTMLInputElement).value;
</script>

{#snippet about(field: SiteFormField)}
	<p class="group-note" id="{id}-{field.field}-help">{field.help}</p>
	{#if editing.fault(field.field)}
		<p class="group-note refusal" role="alert">{editing.fault(field.field)}</p>
	{/if}
{/snippet}

{#snippet line(field: SiteFormField, value: string, set: (typed: string) => void, kind?: 'url')}
	<section class="group">
		<label class="group-title" for="{id}-{field.field}">{field.title}</label>
		<div class="group-card">
			<div class="field">
				<input
					id="{id}-{field.field}"
					type={kind ?? 'text'}
					autocomplete="off"
					aria-describedby="{id}-{field.field}-help"
					aria-required={field.isNeeded}
					aria-invalid={Boolean(editing.fault(field.field))}
					{value}
					oninput={(event) => set(typed(event))}
				/>
			</div>
		</div>
		{@render about(field)}
	</section>
{/snippet}

<form onsubmit={send} novalidate>
	{#each form.fields as field (field.field)}
		{#if field.field === 'name'}
			{@render line(field, draft.name, (name) => editing.set({ name }))}
		{:else if field.field === 'place'}
			<section class="group" aria-labelledby="{id}-place">
				<h3 class="group-title" id="{id}-place">{field.title}</h3>
				<div class="group-card">
					<div class="group-row">
						<span class="numeric">{text.place(draft.position.lat, draft.position.lon)}</span>
						<button class="button-plain move" type="button" onclick={onmove}>Move</button>
					</div>
				</div>
				{@render about(field)}
			</section>
		{:else if field.field === 'uses'}
			<section class="group" aria-labelledby="{id}-uses">
				<h3 class="group-title" id="{id}-uses">{field.title}</h3>
				<div class="chips" role="group" aria-labelledby="{id}-uses">
					{#each form.uses as use (use.kind)}
						<button
							class="chip"
							type="button"
							aria-pressed={draft.uses.includes(use.kind)}
							onclick={() => toggleUse(use.kind)}
						>
							{use.words}
						</button>
					{/each}
				</div>
				{@render about(field)}
			</section>
		{:else if field.field === 'height'}
			<section class="group">
				<label class="group-title" for="{id}-height">{field.title}</label>
				<div class="group-card">
					<div class="field">
						<input
							id="{id}-height"
							class="numeric"
							type="number"
							inputmode="numeric"
							min={form.lowestM}
							max={form.highestM}
							step="1"
							autocomplete="off"
							aria-describedby="{id}-height-help"
							aria-invalid={Boolean(editing.fault('height'))}
							value={height}
							oninput={(event) => typedHeight(typed(event))}
						/>
						<span class="unit">m</span>
					</div>
				</div>
				{@render about(field)}
			</section>
		{:else if field.field === 'wind'}
			<section class="group" aria-labelledby="{id}-wind">
				<h3 class="group-title" id="{id}-wind">{field.title}</h3>
				<div class="chips" class:compact role="group" aria-labelledby="{id}-wind">
					{#each form.winds as wind (wind.fromDeg)}
						<button
							class="chip numeric"
							type="button"
							aria-pressed={draft.windFromDeg.includes(wind.fromDeg)}
							onclick={() => toggleWind(wind.fromDeg)}
						>
							{wind.words}
						</button>
					{/each}
				</div>
				{#if editing.windInWords}
					<p class="group-note said" role="status">{editing.windInWords}</p>
				{/if}
				{@render about(field)}
			</section>
		{:else if field.field === 'description'}
			<section class="group">
				<label class="group-title" for="{id}-description">{field.title}</label>
				<div class="group-card">
					<textarea
						id="{id}-description"
						rows="4"
						aria-describedby="{id}-description-help"
						aria-invalid={Boolean(editing.fault('description'))}
						value={draft.description}
						oninput={(event) => editing.set({ description: typed(event) })}></textarea>
				</div>
				{@render about(field)}
			</section>
		{:else if field.field === 'fee'}
			{@render line(field, draft.fee, (fee) => editing.set({ fee }))}
		{:else if field.field === 'guests'}
			<section class="group">
				<label class="group-title" for="{id}-guests">{field.title}</label>
				<div class="group-card">
					<div class="field">
						<select
							id="{id}-guests"
							aria-describedby="{id}-guests-help"
							value={String(form.guests.findIndex((one) => one.value === draft.guestGuidelines))}
							onchange={(event) =>
								editing.set({ guestGuidelines: form.guests[Number(typed(event))]?.value })}
						>
							{#each form.guests as choice, index (index)}
								<option value={String(index)}>{choice.words}</option>
							{/each}
						</select>
					</div>
				</div>
				{@render about(field)}
			</section>
		{:else if field.field === 'noFlightTime'}
			{@render line(field, draft.noFlightTime, (noFlightTime) => editing.set({ noFlightTime }))}
		{:else if field.field === 'website'}
			{@render line(field, draft.website, (website) => editing.set({ website }), 'url')}
		{:else if field.field === 'comment'}
			{@render line(field, editing.comment, (comment) => editing.say(comment))}
		{/if}
	{/each}

	<section class="group" aria-label={form.licence}>
		<!-- What giving a site means, before it is sent, with the licence to read. -->
		<p class="giving">
			{giving.before}<a href={form.licenceUrl} target="_blank" rel="noopener noreferrer"
				>{form.licence}</a
			>{giving.after}
		</p>
		{#if editing.refusal}
			<div class="group-card refused" role="alert">
				<p>{editing.refusal.words}</p>
				{#if editing.refusal.inTheWay}
					{@const other = editing.refusal.inTheWay}
					<button class="button-plain" type="button" onclick={() => onopen(other.id)}>
						{other.words}
					</button>
				{/if}
			</div>
		{/if}
		{#if editing.changedMeanwhile.length > 0}
			<!-- What the others changed, in the core's words, to look at before sending again. -->
			<ul class="group-card meanwhile" aria-label="How it stands now">
				{#each editing.changedMeanwhile as revision (revision.number)}
					<li>
						<p>{revision.words}</p>
						{#each revision.changes as change (change)}
							<p class="lesser">{change}</p>
						{/each}
					</li>
				{/each}
			</ul>
		{/if}
		<div class="submit">
			<button class="button-prominent" type="submit" disabled={editing.sending}>
				{editing.corrects ? form.sendCorrection : form.sendNew}
			</button>
		</div>
	</section>
</form>

<style>
	form > .group:first-child {
		margin-top: 0.25rem;
	}

	label.group-title {
		display: block;
	}

	.move {
		margin: -0.5rem -0.75rem -0.5rem 0;
	}

	.unit {
		min-width: 0;
		color: var(--text-secondary);
	}

	/* The pack's chips, as the sites' filter has them: ticked ones in the
	   primary colour. They wrap; the sixteen points scroll sideways in one
	   row on a narrow screen. */
	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
		min-width: 0;
	}

	.chips.compact {
		flex-wrap: nowrap;
		overflow-x: auto;
		scrollbar-width: none;
	}

	.chip {
		flex: none;
		min-height: 2.25rem;
		padding: 0 var(--space-3);
		border-radius: var(--radius-pill);
		background: var(--surface);
		box-shadow: inset 0 0 0 1px var(--separator);
		color: var(--text);
		font-size: var(--subheadline);
		font-weight: 600;
		white-space: nowrap;
	}

	.chip[aria-pressed='true'] {
		background: var(--accent);
		color: var(--on-accent);
		box-shadow: none;
	}

	select {
		flex: 1;
		min-width: 0;
		min-height: var(--target);
		padding: 0;
		border: 0;
		background: none;
		color: var(--text);
		font: inherit;
	}

	.field:has(select:focus-visible) {
		outline: 0.1875rem solid var(--color-focus);
		outline-offset: -0.1875rem;
		border-radius: 0.75rem;
	}

	select:focus-visible {
		outline: none;
	}

	textarea {
		display: block;
		width: 100%;
		min-height: 6rem;
		padding: 0.75rem 1rem;
		border: 0;
		background: none;
		color: var(--text);
		font: inherit;
		resize: vertical;
	}

	textarea:focus-visible {
		outline-offset: -0.1875rem;
		border-radius: var(--radius-card);
	}

	/* The wind as the core will send it: said, not a line of help. */
	.said {
		color: var(--text);
	}

	.refusal {
		color: var(--danger);
	}

	.giving {
		padding: 0 1rem;
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	.giving a {
		color: var(--link);
	}

	.refused {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		margin-top: var(--space-3);
		padding: 0.75rem 1rem;
		color: var(--danger);
	}

	.refused .button-plain {
		margin: 0.25rem 0 -0.5rem -0.75rem;
		/* Not a row of the card: no hairline over it. */
		box-shadow: none;
	}

	.meanwhile {
		margin-top: var(--space-3);
		font-size: var(--footnote);
	}

	.meanwhile li {
		padding: 0.75rem 1rem;
	}

	.lesser {
		color: var(--text-secondary);
	}

	.submit {
		display: flex;
		justify-content: flex-end;
		margin-top: 1.25rem;
	}

	@media (hover: hover) {
		.giving a:hover {
			text-decoration: underline;
		}
	}

	/* Compact on a wide screen: controls about 32 px. */
	@media (min-width: 56rem) {
		.chip {
			min-height: 2rem;
		}
	}
</style>
