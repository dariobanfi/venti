<!--
	"I took off here and it was good": the small form with which a pilot
	confirms a flying site, for a day. Every word of it is the core's
	(`siteConfirmForm`): the days as a menu with today chosen, an optional
	line for other pilots with its limit, and, only where the logbook has
	one, the pilot's own flights of that day that began at the site, of
	which one can be named. The request and every refusal are the core's
	too; a refusal stands over the button in the core's words.

	Sending it again takes the place of the pilot's first confirmation,
	which is how its day is changed.
-->
<script lang="ts">
	import type {
		SiteConfirmFlight,
		SiteConfirmForm,
		SiteView,
		SiteWritten
	} from '#lib/core/index.ts';
	import Sheet from '#lib/design/Sheet.svelte';
	import { dayOf } from '#lib/groups/days.ts';
	import { said, type SiteEditsModel } from './edit.svelte';

	interface Props {
		open: boolean;
		site: SiteView;
		edits: SiteEditsModel;
		/** Called with the site as the server has it now, and the core's sentence. */
		onconfirmed: (written: SiteWritten) => void;
		onclose: () => void;
	}

	let { open, site, edits, onconfirmed, onclose }: Props = $props();

	const id = $props.id();

	/** The pilot's own today, as the form was asked for. */
	let today = $state('');
	let form = $state.raw<SiteConfirmForm>();
	let day = $state('');
	let note = $state('');
	/** The pilot's flights of the chosen day that began here. Often none. */
	let flights = $state.raw<SiteConfirmFlight[]>([]);
	/** The flight named with the confirmation; empty for none. */
	let flight = $state('');
	let sending = $state(false);
	let refusal = $state.raw<string>();

	// A form that opens starts on today, empty.
	$effect(() => {
		if (!open) return;
		today = dayOf(new Date());
		day = today;
		note = '';
		flight = '';
		flights = [];
		refusal = undefined;
		form = undefined;
		edits.confirmForm(today).then(
			(known) => (form = known),
			(error) => (refusal = said(error))
		);
	});

	// The flights of the day chosen, asked of the core's logbook.
	$effect(() => {
		if (!open || day === '') return;
		const of = day;
		const here = $state.snapshot(site) as SiteView;
		// How far the pilot's clock is ahead of UTC on that day, in seconds.
		const utcOffsetS = -new Date(`${of}T12:00:00`).getTimezoneOffset() * 60;
		let current = true;
		edits.confirmFlights(here, of, utcOffsetS).then(
			(found) => {
				if (!current) return;
				flights = found;
				if (!found.some((one) => one.id === flight)) flight = '';
			},
			() => {
				if (current) flights = [];
			}
		);
		return () => {
			current = false;
		};
	});

	async function send(event: SubmitEvent) {
		event.preventDefault();
		if (sending) return;
		sending = true;
		refusal = undefined;
		try {
			onconfirmed(
				await edits.confirm(site.id, day, today, note, flight === '' ? undefined : flight)
			);
		} catch (error) {
			refusal = said(error);
		} finally {
			sending = false;
		}
	}
</script>

<Sheet {open} title={form?.title ?? ''} {onclose}>
	{#if form}
		<form onsubmit={send} novalidate>
			<p class="group-note about">{form.about}</p>
			<section class="group">
				<label class="group-title" for="{id}-day">{form.dayTitle}</label>
				<div class="group-card">
					<div class="field">
						<select id="{id}-day" bind:value={day} onchange={() => (refusal = undefined)}>
							{#each form.days as one (one.day)}
								<option value={one.day}>{one.words}</option>
							{/each}
						</select>
					</div>
				</div>
			</section>
			<section class="group">
				<label class="group-title" for="{id}-note">{form.noteTitle}</label>
				<div class="group-card">
					<div class="field">
						<input
							id="{id}-note"
							type="text"
							autocomplete="off"
							maxlength={form.noteAtMost}
							aria-describedby="{id}-note-help"
							bind:value={note}
							oninput={() => (refusal = undefined)}
						/>
					</div>
				</div>
				<p class="group-note" id="{id}-note-help">{form.noteHelp}</p>
			</section>
			{#if flights.length > 0}
				<section class="group">
					<label class="group-title" for="{id}-flight">{form.flightTitle}</label>
					<div class="group-card">
						<div class="field">
							<select
								id="{id}-flight"
								aria-describedby="{id}-flight-help"
								bind:value={flight}
								onchange={() => (refusal = undefined)}
							>
								<option value="">{form.noFlight}</option>
								{#each flights as one (one.id)}
									<option value={one.id}>{one.words}</option>
								{/each}
							</select>
						</div>
					</div>
					<p class="group-note" id="{id}-flight-help">{form.flightHelp}</p>
				</section>
			{/if}
			{#if refusal}
				<p class="group-note refusal" role="alert">{refusal}</p>
			{/if}
			<div class="submit">
				<button class="button-prominent" type="submit" disabled={sending}>{form.send}</button>
			</div>
		</form>
	{:else if refusal}
		<p class="group-note refusal" role="alert">{refusal}</p>
	{:else}
		<p class="group-note" role="status">Opening…</p>
	{/if}
</Sheet>

<style>
	.about {
		margin-top: 0.25rem;
	}

	label.group-title {
		display: block;
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

	.refusal {
		color: var(--danger);
	}

	.submit {
		display: flex;
		justify-content: flex-end;
		margin-top: 1.25rem;
	}
</style>
