<!--
	The form for an open plan: a new one, or the pilot's own to change. It is
	the group plan's form (`groups/PlanForm.svelte`) with three differences
	(docs/design.md, "Open plans"): the place is marked on a small map, or
	taken from the take-offs near; the seats free in the pilot's car, or
	"Looking for company"; and a short note. Above "Post" the core's words
	say that everyone can read it, with the pilot's handle and name.

	A change sends the plan whole. What a plan may hold is the core's to say:
	a refusal stands under the form, in its words, and goes when the pilot
	changes the plan.
-->
<script lang="ts">
	import { untrack } from 'svelte';
	import type { Coordinate, OpenPlanDraft, OpenPlanView, TodayTakeoff } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import Segmented from '#lib/design/Segmented.svelte';
	import Sheet from '#lib/design/Sheet.svelte';
	import { dayOf } from '#lib/groups/days.ts';
	import type { OpenPlansModel } from './open-plans.svelte';
	import PlaceMap from './PlaceMap.svelte';

	interface Props {
		open: boolean;
		model: OpenPlansModel;
		/** Where a new plan's map opens and whose take-offs near are offered. */
		near: Coordinate;
		/** The plan to change. Without one the form is for a new plan. */
		editing?: OpenPlanView;
		onclose: () => void;
		/** Told of the plan as the server took it. */
		onsaved?: (plan: OpenPlanView) => void;
	}

	let { open, model, near, editing, onclose, onsaved }: Props = $props();

	const id = $props.id();
	const text = format();

	const SEATS = 'Seats free';
	const COMPANY = 'Looking for company';
	const SEAT_COUNTS = [1, 2, 3, 4, 5, 6, 7, 8];

	let place = $state('');
	/** The place under the map's mark. */
	let position = $state.raw<Coordinate>({ lat: 0, lon: 0 });
	/** Where the map opened, for the time the form is open. */
	let start = $state.raw<Coordinate>({ lat: 0, lon: 0 });
	let day = $state('');
	let hasTime = $state(false);
	let time = $state('');
	let offer = $state(COMPANY);
	let seats = $state(1);
	let note = $state('');
	let saving = $state(false);
	/** Why the plan was not saved, in words. */
	let problem = $state.raw<string>();
	/** The earliest day the field offers: the pilot's own today. */
	let today = $state('');
	/** The take-offs around the place, to take a plan's place from. */
	let takeoffs = $state.raw<TodayTakeoff[]>([]);
	/** The take-off chosen, by its identifier. */
	let takeoff = $state('');
	let mapView = $state<PlaceMap>();

	// A form that opens starts from the plan being changed, or from today
	// near the place. Only opening fills it: the list may be asked for
	// again while the pilot types.
	$effect(() => {
		if (!open) return;
		let current = true;
		untrack(() => {
			today = dayOf(new Date());
			problem = undefined;
			saving = false;
			place = editing?.place ?? '';
			start = editing?.position ?? near;
			position = start;
			day = editing?.day ?? today;
			hasTime = Boolean(editing?.meetAt);
			time = editing?.meetAt ?? '';
			offer = editing?.seats !== undefined ? SEATS : COMPANY;
			seats = editing?.seats ?? 1;
			note = editing?.note ?? '';
			takeoff = '';
			model.takeoffsNear(start).then(
				(found) => {
					if (current) takeoffs = found;
				},
				() => {
					if (current) takeoffs = [];
				}
			);
		});
		return () => {
			current = false;
		};
	});

	/** A take-off chosen from the list: its name and its place become the plan's. */
	function choose(chosen: string) {
		takeoff = chosen;
		const one = takeoffs.find((each) => each.site.id === chosen);
		if (!one) return;
		place = one.site.name;
		position = one.site.position;
		mapView?.show(one.site.position);
		changed();
	}

	async function save(event: SubmitEvent) {
		event.preventDefault();
		if (saving) return;
		saving = true;
		problem = undefined;
		const draft: OpenPlanDraft = {
			day,
			place,
			position,
			meetAt: hasTime && time ? time : undefined,
			seats: offer === SEATS ? seats : undefined,
			note
		};
		try {
			const saved = editing ? await model.change(editing.id, draft) : await model.post(draft);
			onclose();
			onsaved?.(saved);
		} catch (error) {
			problem = error instanceof Error ? error.message : String(error);
		} finally {
			saving = false;
		}
	}

	const changed = () => (problem = undefined);
</script>

<Sheet {open} title={editing ? 'Change the open plan' : 'New open plan'} {onclose}>
	<form onsubmit={save} oninput={changed} novalidate>
		<section class="group first" aria-label="Where">
			<div class="group-card">
				<label class="field">
					<span>Where</span>
					<input
						name="place"
						autocomplete="off"
						placeholder="The name of the place"
						bind:value={place}
					/>
				</label>
				{#if takeoffs.length > 0}
					<label class="field">
						<span id="{id}-takeoff">Take-off</span>
						<select
							aria-labelledby="{id}-takeoff"
							value={takeoff}
							onchange={(event) => choose(event.currentTarget.value)}
						>
							<option value="" disabled>Choose one near</option>
							{#each takeoffs as one (one.site.id)}
								<option value={one.site.id}>{one.site.name}</option>
							{/each}
						</select>
					</label>
				{/if}
			</div>
			{#if open}
				<div class="on-map">
					<PlaceMap
						bind:this={mapView}
						{start}
						label="Map of the plan’s place"
						onmove={(middle) => {
							position = middle;
							changed();
						}}
					/>
				</div>
			{/if}
			<p class="group-note numeric" aria-live="polite">
				{text.place(position.lat, position.lon)}
			</p>
		</section>

		<section class="group" aria-label="When">
			<div class="group-card">
				<label class="field">
					<span>Day</span>
					<input type="date" min={today} bind:value={day} />
				</label>
				<label class="group-row toggle">
					<span>Set a time</span>
					<input type="checkbox" role="switch" bind:checked={hasTime} />
				</label>
				{#if hasTime}
					<label class="field">
						<span>Time</span>
						<input type="time" bind:value={time} />
					</label>
				{/if}
			</div>
		</section>

		<section class="group" aria-label="What you offer">
			<Segmented label="What you offer" options={[SEATS, COMPANY]} bind:value={offer} />
			{#if offer === SEATS}
				<div class="group-card seats">
					<label class="field">
						<span id="{id}-seats">Seats</span>
						<select aria-labelledby="{id}-seats" bind:value={seats}>
							{#each SEAT_COUNTS as count (count)}
								<option value={count}>{count}</option>
							{/each}
						</select>
					</label>
				</div>
			{/if}
		</section>

		<section class="group">
			<label class="group-title" for="{id}-note">Note</label>
			<div class="group-card">
				<textarea
					id="{id}-note"
					rows="3"
					placeholder="Anything else"
					aria-describedby="{id}-who"
					bind:value={note}></textarea>
			</div>
			<p class="group-note" id="{id}-who">{model.publicWords ?? ''}</p>
			{#if problem}
				<p class="group-note refusal" role="alert">{problem}</p>
			{/if}
		</section>

		<div class="submit">
			<button class="button-prominent" type="submit" disabled={saving || place.trim() === ''}>
				{saving ? 'Saving…' : editing ? 'Save' : 'Post'}
			</button>
		</div>
	</form>
</Sheet>

<style>
	.first {
		margin-top: 0.25rem;
	}

	label.group-title {
		display: block;
	}

	.on-map {
		margin-top: var(--space-3);
	}

	.seats {
		margin-top: var(--space-3);
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

	/* The browser draws a date and a time with a box of its own. */
	input[type='date'],
	input[type='time'] {
		font-family: var(--font-rounded);
		font-variant-numeric: tabular-nums;
	}

	textarea {
		display: block;
		width: 100%;
		min-height: 5rem;
		padding: 0.75rem 1rem;
		border: 0;
		background: none;
		color: var(--text);
		font: inherit;
		resize: vertical;
	}

	textarea::placeholder {
		color: var(--text-secondary);
	}

	textarea:focus-visible {
		outline-offset: -0.1875rem;
		border-radius: var(--radius-card);
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
