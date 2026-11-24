<!--
	The form for a new group, or for changing one (its owner's and its
	admins'): the name, a short description, whether it is public (a new
	group only: the owner switches it afterwards on the group's page), and
	its home, a point on the map with a radius and the place in a few words,
	by which pilots near find it (owner decision 26). What the server would
	refuse, the core refuses first, in its words.

	The map is the search's (`AreaMap.svelte`, its "radius" mode), at its
	own height and opened on the home and its circle, a new group's too: the
	mark is dragged or the map tapped where it goes, the handle on the
	circle's edge drags the radius.
-->
<script lang="ts">
	import { messageOf } from '#lib/core/index.ts';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { untrack } from 'svelte';
	import { goto } from '$app/navigation';
	import { groupHref } from '#lib/app/routes.ts';
	import AreaMap from '#lib/community/AreaMap.svelte';
	import type { CommunityDraft, CommunityView, Coordinate } from '#lib/core/index.ts';
	import Segmented from '#lib/design/Segmented.svelte';
	import Sheet from '#lib/design/Sheet.svelte';
	import { FIRST_REGION } from '#lib/today/model.svelte.ts';
	import type { CommunitiesModel } from './communities.svelte';

	interface Props {
		open: boolean;
		model: CommunitiesModel;
		/** The group to change; without one, the form makes a new group. */
		editing?: CommunityView;
		/** Changes the group to the draft; given with `editing`. Throws why not, in words. */
		onchange?: (draft: CommunityDraft) => Promise<void>;
		onclose: () => void;
	}

	let { open, model, editing, onchange, onclose }: Props = $props();

	const app = getApp();

	const id = $props.id();
	const PUBLIC = $derived(app.said?.community.public ?? 'Public');
	const PRIVATE = $derived(app.said?.community.private ?? 'Private');

	let name = $state('');
	let description = $state('');
	let kind = $state('');
	let hasHome = $state(false);
	let centre = $state.raw<Coordinate>(FIRST_REGION);
	let radiusKm = $state(20);
	let place = $state('');
	let saving = $state(false);
	let problem = $state<string>();

	// The form starts from the group as it is, or empty, each time it opens.
	$effect(() => {
		if (!open) return;
		untrack(() => {
			name = editing?.name ?? '';
			description = editing?.description ?? '';
			kind = editing?.isPublic === false ? PRIVATE : PUBLIC;
			hasHome = Boolean(editing?.center) || !editing;
			centre = editing?.center ?? FIRST_REGION;
			radiusKm = clamp(editing?.radiusKm ?? 20);
			place = editing?.place ?? '';
			saving = false;
			problem = undefined;
		});
	});

	const clamp = (km: number) => Math.min(100, Math.max(5, Math.round(km / 5) * 5));

	async function save(event: SubmitEvent) {
		event.preventDefault();
		if (saving) return;
		saving = true;
		problem = undefined;
		const draft: CommunityDraft = {
			name,
			description,
			home: hasHome ? { center: centre, radiusKm, place } : undefined
		};
		try {
			if (editing && onchange) {
				await onchange(draft);
				onclose();
			} else {
				const made = await model.create(draft, kind === PUBLIC);
				onclose();
				await goto(groupHref(made.group.id));
			}
		} catch (error) {
			problem = messageOf(error);
		} finally {
			saving = false;
		}
	}
</script>

<Sheet {open} title={editing ? 'Change the group' : 'New group'} {onclose}>
	<form onsubmit={save} oninput={() => (problem = undefined)} novalidate>
		<section class="group first" aria-label="The group">
			<div class="group-card">
				<label class="field">
					<span>Name</span>
					<input name="name" autocomplete="off" bind:value={name} />
				</label>
			</div>
		</section>

		<section class="group">
			<label class="group-title" for="{id}-about">What the group is about</label>
			<div class="group-card">
				<textarea id="{id}-about" rows="3" bind:value={description}></textarea>
			</div>
		</section>

		{#if !editing}
			<section class="group">
				<Segmented label="Who finds it" options={[PUBLIC, PRIVATE]} bind:value={kind} />
				<p class="group-note">
					{kind === PUBLIC
						? 'Anyone signed in finds, reads and joins it.'
						: 'Only its members read it; you and its admins invite pilots by handle.'}
				</p>
			</section>
		{/if}

		<section class="group">
			<div class="group-card">
				<label class="group-row toggle">
					<span>Home</span>
					<input type="checkbox" role="switch" bind:checked={hasHome} />
				</label>
				{#if hasHome}
					<label class="field">
						<span>The place, such as Inn valley</span>
						<input name="place" autocomplete="off" bind:value={place} />
					</label>
				{/if}
			</div>
			{#if hasHome && open}
				<div class="map">
					<AreaMap
						mode="radius"
						{centre}
						{radiusKm}
						corners={[]}
						closed={false}
						maxCorners={0}
						frame
						label="Map: the group's home and its radius"
						onplace={(at) => (centre = at)}
						onradius={(km) => (radiusKm = clamp(km))}
						oncorner={() => {}}
						onclose={() => {}}
					/>
				</div>
				<div class="radius">
					<label for="{id}-radius">Radius</label>
					<input id="{id}-radius" type="range" min="5" max="100" step="5" bind:value={radiusKm} />
					<output class="numeric" for="{id}-radius">{radiusKm} km around</output>
				</div>
			{/if}
			<p class="group-note">Pilots near it find the group.</p>
			{#if problem}
				<p class="group-note refusal" role="alert">{problem}</p>
			{/if}
		</section>

		<div class="submit">
			<button class="button-prominent" type="submit" disabled={saving || name.trim() === ''}>
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

	textarea {
		display: block;
		width: 100%;
		min-height: 4rem;
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

	/* The map takes its own height: a box lower than it cut off its foot,
	   the credits with it, and put the home above the box's middle. */
	.map {
		margin-top: var(--space-3);
	}

	.radius {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		min-height: var(--target);
		padding: 0 var(--space-1);
	}

	.radius label {
		color: var(--text-secondary);
	}

	.radius input {
		flex: 1;
		min-width: 0;
		min-height: var(--target);
		accent-color: var(--accent);
	}

	.radius output {
		min-width: 4.5rem;
		text-align: end;
		font-weight: 600;
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
