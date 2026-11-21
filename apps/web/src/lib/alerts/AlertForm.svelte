<!--
	The form of an alarm: a wind alarm for a station (the directions the wind
	is to come from, the least and the most wind, a limit for the gusts, the
	hours), or a day alarm for a place (the days ahead and the least kind of
	day). Over it the core's line of what the alarm waits for, as it changes
	(`alertLine`); the core's faults stand at their fields once the pilot has
	tried to save (`alertCheck`), and the server's refusal under the form.

	The words of the choices are the app's: the core has none for them yet.
-->
<script lang="ts">
	import { untrack } from 'svelte';
	import type { AlertDayKind, AlertDraft, AlertFaultView } from '#lib/core/index.ts';
	import Sheet from '#lib/design/Sheet.svelte';
	import type { AlertsModel } from './model.svelte';

	interface Props {
		open: boolean;
		alerts: AlertsModel;
		/** The alarm being changed; nothing for a new one. */
		id?: string;
		/** What the form starts from. */
		start: AlertDraft;
		onclose: () => void;
	}

	let { open, alerts, id, start, onclose }: Props = $props();
	const uid = $props.id();

	// Filled when the form opens, below.
	let draft = $state<AlertDraft>(
		untrack(() => structuredClone($state.snapshot(start) as AlertDraft))
	);
	let points = $state.raw<string[]>([]);
	let line = $state('');
	let faults = $state.raw<AlertFaultView[]>([]);
	/** Whether the pilot has tried to save: the faults show from then on. */
	let tried = $state(false);
	let sending = $state(false);
	/** Why it was not saved or removed, in words. */
	let refusal = $state.raw<string>();

	// Only opening fills the form.
	$effect(() => {
		if (!open) return;
		untrack(() => {
			draft = structuredClone($state.snapshot(start) as AlertDraft);
			tried = false;
			sending = false;
			refusal = undefined;
		});
		void alerts.points().then(
			(said) => (points = said),
			() => {}
		);
	});

	// The core's line and faults, as the draft changes; a late answer is dropped.
	let asked = 0;
	$effect(() => {
		const now = $state.snapshot(draft) as AlertDraft;
		const mine = ++asked;
		void Promise.all([alerts.line(now), alerts.check(now)]).then(
			([said, found]) => {
				if (mine !== asked) return;
				line = said;
				faults = found;
			},
			() => {}
		);
	});

	/** The faults at a field, by the names the core gives them. */
	function at(...fields: string[]): AlertFaultView[] {
		return tried ? faults.filter((fault) => fields.includes(fault.field)) : [];
	}

	const PLACED = [
		'name',
		'rule.station',
		'rule.station_name',
		'rule.place',
		'rule.place_name',
		'rule.directions',
		'rule.min_kmh',
		'rule.max_kmh',
		'rule.gusts_max_kmh',
		'rule.hours',
		'rule.days_ahead'
	];
	const elsewhere = $derived(tried ? faults.filter((fault) => !PLACED.includes(fault.field)) : []);

	/** A whole number from a field, or nothing for an empty one. */
	function whole(value: string, most: number): number | undefined {
		const trimmed = value.trim();
		if (trimmed === '') return undefined;
		const number = Number.parseInt(trimmed, 10);
		return Number.isNaN(number) ? undefined : Math.min(Math.max(number, 0), most);
	}

	type Numbered = 'minKmh' | 'maxKmh' | 'gustsMaxKmh' | 'fromHour' | 'toHour';
	function set(field: Numbered, value: string) {
		draft[field] = whole(value, field.endsWith('Hour') ? 99 : 9999);
	}

	function from(point: number | undefined) {
		draft.fromPoint = point;
		draft.toPoint = point === undefined ? undefined : (draft.toPoint ?? point);
	}

	function toggleDay(day: number) {
		draft.daysAhead = draft.daysAhead.includes(day)
			? draft.daysAhead.filter((one) => one !== day)
			: [...draft.daysAhead, day].sort();
	}

	const DAYS = ['Today', 'Tomorrow', 'In 2 days', 'In 3 days'];
	const KINDS: { kind: AlertDayKind; words: string }[] = [
		{ kind: 'flyable', words: 'Flyable' },
		{ kind: 'local', words: 'Local thermals' },
		{ kind: 'crossCountry', words: 'Cross-country' },
		{ kind: 'big', words: 'Big day' }
	];

	async function save(event: SubmitEvent) {
		event.preventDefault();
		tried = true;
		refusal = undefined;
		if (sending) return;
		const found = await alerts.check(draft);
		faults = found;
		if (found.length > 0) return;
		sending = true;
		try {
			await alerts.save(id, draft);
			onclose();
		} catch (error) {
			refusal = (error as Error).message;
		} finally {
			sending = false;
		}
	}

	async function remove() {
		if (!id || sending) return;
		sending = true;
		refusal = undefined;
		try {
			await alerts.remove(id);
			onclose();
		} catch (error) {
			refusal = (error as Error).message;
		} finally {
			sending = false;
		}
	}
</script>

{#snippet said(found: AlertFaultView[])}
	{#each found as fault (fault.field + fault.message)}
		<p class="group-note refusal" role="alert">{fault.message}</p>
	{/each}
{/snippet}

<Sheet {open} title={id ? 'Change the alarm' : 'Alert me'} {onclose}>
	<form onsubmit={save} novalidate>
		<p class="summary" aria-live="polite" data-alert-line>{line}</p>

		<div class="group">
			<div class="group-card">
				<label class="field">
					<span>Name</span>
					<input name="alert-name" autocomplete="off" bind:value={draft.name} />
				</label>
				<div class="group-row">
					<span>{draft.kind === 'wind' ? 'Station' : 'Place'}</span>
					<span class="group-value">
						{draft.kind === 'wind' ? draft.stationName : draft.placeName}
					</span>
				</div>
				<label class="group-row toggle">
					<span>On</span>
					<input type="checkbox" role="switch" bind:checked={draft.enabled} />
				</label>
			</div>
			{@render said(
				at('name', 'rule.station', 'rule.station_name', 'rule.place', 'rule.place_name')
			)}
		</div>

		{#if draft.kind === 'wind'}
			<section class="group" aria-labelledby="{uid}-from">
				<h3 class="group-title" id="{uid}-from">Wind from</h3>
				<div class="chips" role="group" aria-labelledby="{uid}-from">
					<button
						class="chip"
						type="button"
						aria-pressed={draft.fromPoint === undefined}
						onclick={() => from(undefined)}>Any</button
					>
					{#each points as point, index (point)}
						<button
							class="chip"
							type="button"
							aria-pressed={draft.fromPoint === index}
							onclick={() => from(index)}>{point}</button
						>
					{/each}
				</div>
				{#if draft.fromPoint !== undefined}
					<h3 class="group-title to" id="{uid}-to">To</h3>
					<div class="chips" role="group" aria-labelledby="{uid}-to">
						{#each points as point, index (point)}
							<button
								class="chip"
								type="button"
								aria-pressed={draft.toPoint === index}
								onclick={() => (draft.toPoint = index)}>{point}</button
							>
						{/each}
					</div>
				{/if}
				{@render said(at('rule.directions'))}
			</section>

			<section class="group" aria-labelledby="{uid}-speed">
				<h3 class="group-title" id="{uid}-speed">Wind, km/h</h3>
				<div class="group-card">
					<label class="field">
						<span>Least</span>
						<input
							inputmode="numeric"
							autocomplete="off"
							value={draft.minKmh ?? ''}
							oninput={(event) => set('minKmh', event.currentTarget.value)}
						/>
					</label>
					<label class="field">
						<span>Most</span>
						<input
							inputmode="numeric"
							autocomplete="off"
							value={draft.maxKmh ?? ''}
							oninput={(event) => set('maxKmh', event.currentTarget.value)}
						/>
					</label>
					<label class="field">
						<span>Gusts at most</span>
						<input
							inputmode="numeric"
							autocomplete="off"
							value={draft.gustsMaxKmh ?? ''}
							oninput={(event) => set('gustsMaxKmh', event.currentTarget.value)}
						/>
					</label>
				</div>
				{@render said(at('rule.min_kmh', 'rule.max_kmh', 'rule.gusts_max_kmh'))}
			</section>

			<section class="group" aria-labelledby="{uid}-hours">
				<h3 class="group-title" id="{uid}-hours">Hours, Central European Time</h3>
				<div class="group-card">
					<label class="field">
						<span>From</span>
						<input
							inputmode="numeric"
							autocomplete="off"
							placeholder="0"
							value={draft.fromHour ?? ''}
							oninput={(event) => set('fromHour', event.currentTarget.value)}
						/>
					</label>
					<label class="field">
						<span>To</span>
						<input
							inputmode="numeric"
							autocomplete="off"
							placeholder="24"
							value={draft.toHour ?? ''}
							oninput={(event) => set('toHour', event.currentTarget.value)}
						/>
					</label>
				</div>
				{@render said(at('rule.hours'))}
			</section>
		{:else}
			<section class="group" aria-labelledby="{uid}-days">
				<h3 class="group-title" id="{uid}-days">Days</h3>
				<div class="chips" role="group" aria-labelledby="{uid}-days">
					{#each DAYS as day, index (day)}
						<button
							class="chip"
							type="button"
							aria-pressed={draft.daysAhead.includes(index)}
							onclick={() => toggleDay(index)}>{day}</button
						>
					{/each}
				</div>
				{@render said(at('rule.days_ahead'))}
			</section>

			<section class="group" aria-labelledby="{uid}-kind">
				<h3 class="group-title" id="{uid}-kind">At least</h3>
				<div class="chips" role="group" aria-labelledby="{uid}-kind">
					{#each KINDS as one (one.kind)}
						<button
							class="chip"
							type="button"
							aria-pressed={draft.atLeast === one.kind}
							onclick={() => (draft.atLeast = one.kind)}>{one.words}</button
						>
					{/each}
				</div>
			</section>
		{/if}

		{@render said(elsewhere)}
		{#if refusal}
			<p class="group-note refusal" role="alert">{refusal}</p>
		{/if}

		<div class="submit">
			{#if id}
				<button class="remove" type="button" disabled={sending} onclick={remove}>
					Delete alarm
				</button>
			{/if}
			<button class="button-prominent" type="submit" disabled={sending}>Save</button>
		</div>
	</form>
</Sheet>

<style>
	.summary {
		margin: var(--space-1) 0 var(--space-4);
		padding: var(--space-3) var(--space-4);
		border-radius: var(--radius-card);
		background: var(--surface);
		font-weight: 600;
	}

	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
		padding: 0 var(--space-4);
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

	.to {
		margin-top: var(--space-3);
	}

	.field input {
		font-variant-numeric: tabular-nums;
	}

	.refusal {
		color: var(--danger);
	}

	.submit {
		display: flex;
		justify-content: flex-end;
		gap: var(--space-3);
		margin-top: var(--space-5);
	}

	.remove {
		min-height: var(--target);
		padding: 0 var(--space-4);
		border-radius: var(--radius-control);
		color: var(--danger);
	}

	@media (min-width: 56rem) {
		.chip {
			min-height: 2rem;
		}
	}

	@media (hover: hover) {
		.remove:hover {
			background: var(--fill);
		}
	}
</style>
