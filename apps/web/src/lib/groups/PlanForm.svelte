<!--
	The form for a plan: a new one, or the pilot's own to change. Where (a
	name the pilot types), the day, a time if wanted, a note; with more than
	one group, which. The iPhone app's `PlanForm` is the same form.

	The day and the time are written as the server keeps them, a date on the
	calendar of the place and a time on its clock, which is what a browser's
	date and time fields hand over. What a plan may hold is the core's to
	say: a refusal stands under the form, in its words, and goes when the
	pilot changes the plan.
-->
<script lang="ts">
	import { messageOf } from '#lib/core/index.ts';
	import { untrack } from 'svelte';
	import { getApp } from '#lib/app/app.svelte.ts';
	import type { Coordinate, PlanDraft, PlanView } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import Sheet from '#lib/design/Sheet.svelte';
	import { dayOf } from './days';
	import type { GroupsModel } from './model.svelte';

	interface Props {
		open: boolean;
		groups: GroupsModel;
		/** The plan to change. Without one the form is for a new plan. */
		editing?: PlanView;
		/** The group a new plan goes to, as from a group's chat; else the pilot chooses. */
		inGroup?: string;
		/** Whether that group is public, where the page knows; else the pilot's groups say. */
		inPublic?: boolean;
		/** Told once the plan is posted or changed. */
		onsaved?: () => void;
		onclose: () => void;
	}

	let { open, groups, editing, inGroup, inPublic, onsaved, onclose }: Props = $props();

	const id = $props.id();
	const text = format();
	const app = getApp();

	let group = $state('');
	let place = $state('');
	/**
	 * Where the place is on the map, for a plan that says so. A browser has
	 * no waypoints to take a position from, so the form gives a plan none;
	 * it keeps the one a plan came with, until the pilot takes it off.
	 */
	let position = $state.raw<Coordinate>();
	let day = $state('');
	let hasTime = $state(false);
	let time = $state('');
	let note = $state('');
	let saving = $state(false);
	/** Why the plan was not saved, in words. */
	let problem = $state.raw<string>();
	/** The earliest day the field offers: the pilot's own today. */
	let today = $state('');

	// A form that opens starts from the plan being changed, or from today
	// in the first group. Only opening fills it: the board may be asked for
	// again while the pilot types.
	$effect(() => {
		if (!open) return;
		untrack(() => {
			today = dayOf(new Date());
			problem = undefined;
			saving = false;
			group = editing?.groupId ?? inGroup ?? groups.groups[0]?.id ?? '';
			place = editing?.place ?? '';
			position = editing?.position;
			day = editing?.day ?? today;
			hasTime = Boolean(editing?.meetAt);
			time = editing?.meetAt ?? '';
			note = editing?.note ?? '';
		});
	});

	async function save(event: SubmitEvent) {
		event.preventDefault();
		if (saving) return;
		saving = true;
		problem = undefined;
		const draft: PlanDraft = {
			day,
			place,
			position,
			meetAt: hasTime && time ? time : undefined,
			note
		};
		try {
			if (editing) await groups.change(editing, draft);
			else await groups.post(group, draft);
			onsaved?.();
			onclose();
		} catch (error) {
			problem = messageOf(error);
		} finally {
			saving = false;
		}
	}

	const changed = () => (problem = undefined);

	/* Who sees the plan, in the core's words for its group: public or private. */
	const communities = $derived(app.communities);
	$effect(() => {
		if (open && inPublic === undefined && communities && !untrack(() => communities.mine)) {
			void communities.load();
		}
	});
	const isPublic = $derived(
		(group === inGroup ? inPublic : undefined) ??
			communities?.mine?.find((each) => each.id === group)?.isPublic ??
			false
	);
	const whoSees = $derived(app.said?.whoSees.thisPlan[isPublic ? 'public' : 'private']);
</script>

<Sheet {open} title={editing ? 'Change the plan' : 'New plan'} {onclose}>
	<form onsubmit={save} oninput={changed} novalidate>
		<section class="group first" aria-label="The plan">
			<div class="group-card">
				{#if !editing && !inGroup && groups.groups.length > 1}
					<label class="field">
						<span>Group</span>
						<select bind:value={group}>
							{#each groups.groups as choice (choice.id)}
								<option value={choice.id}>{choice.name}</option>
							{/each}
						</select>
					</label>
				{/if}
				<label class="field">
					<span>Where</span>
					<input
						name="place"
						autocomplete="off"
						placeholder="The name of the place"
						bind:value={place}
					/>
				</label>
				{#if position}
					<div class="group-row">
						<span>On the map</span>
						<span class="where">
							<span class="group-value numeric">{text.place(position.lat, position.lon)}</span>
							<button
								class="off"
								type="button"
								aria-label="Take the position off"
								title="Take the position off"
								onclick={() => {
									position = undefined;
									changed();
								}}
							>
								<Icon name="close" />
							</button>
						</span>
					</div>
				{/if}
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
			<p class="group-note" id="{id}-who">{whoSees}</p>
			{#if problem}
				<p class="group-note refusal" role="alert">{problem}</p>
			{/if}
		</section>

		<div class="submit">
			<button class="button-prominent" type="submit" disabled={saving || place.trim() === ''}>
				{saving ? 'Saving…' : 'Save'}
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

	.where {
		display: flex;
		align-items: center;
		gap: 0.25rem;
		min-width: 0;
	}

	.off {
		flex: none;
		display: grid;
		place-items: center;
		width: var(--target);
		height: var(--target);
		margin: -0.5rem -0.625rem -0.5rem 0;
		border-radius: 50%;
		color: var(--text-secondary);
		font-size: 0.75rem;
	}

	@media (hover: hover) {
		.off:hover {
			background: var(--fill);
		}
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
