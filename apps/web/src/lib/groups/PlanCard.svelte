<!--
	One plan as a card of Community: its day and time at the top, the place
	with the pack's site sign, its group, the faces of who is in, and the
	pilot's own word on it, "I am in", a button that stays pressed while
	the pilot is in. The place leads to the plan's page, where everything
	else is. Who is in is the core's sentence, under the faces.

	A plan carries no picture of its place: a map for each card would be a
	map engine for each, and most plans made on the web have no position.
	What another pilot wrote, a place or a group's name, is set as text.
-->
<script lang="ts">
	import { planHref } from '#lib/app/routes.ts';
	import type { PlanView } from '#lib/core/index.ts';
	import Icon from '#lib/design/Icon.svelte';
	import { facesOf } from './faces';
	import type { GroupsModel } from './model.svelte';

	interface Props {
		plan: PlanView;
		/** What the day is called: "Today", "Tomorrow", "Fri 21 Nov". */
		day: string;
		groups: GroupsModel;
		/** The name the pilot of this browser is shown by, for their face. */
		myName: string | undefined;
	}

	let { plan, day, groups, myName }: Props = $props();

	const shown = $derived(facesOf(plan.pilotsIn, plan.amIn, myName));
	let busy = $state(false);
	/** Why the pilot's word did not go through, in words. */
	let problem = $state<string>();

	async function toggle() {
		busy = true;
		problem = undefined;
		try {
			await groups.setIn(plan, !plan.amIn);
		} catch (error) {
			problem = error instanceof Error ? error.message : String(error);
		} finally {
			busy = false;
		}
	}
</script>

<li class="plan" aria-label="{plan.place}, {day}">
	<div class="when">
		<span class="day">{day}</span>
		{#if plan.meetAt}
			<span class="time numeric">{plan.meetAt}</span>
		{/if}
	</div>
	<a class="place" href={planHref(plan.id)}>
		<span class="sign"><Icon name="pin" /></span>
		<span class="name">{plan.place}</span>
	</a>
	{#if plan.groupName}
		<span class="group">{plan.groupName}</span>
	{/if}
	<div class="who">
		{#if shown.faces.length > 0}
			<ul class="faces" aria-label="Who is in">
				{#each shown.faces as face, index (index)}
					<li class:me={face.isMe} title={face.name}>
						<span aria-hidden="true">{face.initials}</span>
						<span class="visually-hidden">{face.name}</span>
					</li>
				{/each}
				{#if shown.more > 0}
					<li class="more numeric"><span>+{shown.more}</span></li>
				{/if}
			</ul>
		{/if}
		<span class="in-words">{plan.inWords}</span>
	</div>
	<button
		class="in"
		class:pressed={plan.amIn}
		type="button"
		aria-pressed={plan.amIn}
		disabled={busy}
		onclick={toggle}
	>
		{#if plan.amIn}<Icon name="check" />{/if}
		<span>I am in</span>
	</button>
	{#if problem}
		<p class="problem" role="alert">{problem}</p>
	{/if}
</li>

<style>
	.plan {
		position: relative;
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		min-width: 0;
		padding: var(--space-3) var(--space-4) var(--space-4);
		background: var(--surface);
		border: 1px solid var(--separator);
		border-radius: var(--radius-card);
	}

	/* The day leads, as a calendar's page does; the time at its right. */
	.when {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: var(--space-2);
	}

	.day {
		font-size: var(--subheadline);
		font-weight: 600;
		color: var(--link);
	}

	.time {
		font-size: var(--number);
		font-weight: 600;
		line-height: 1.1;
	}

	.place {
		display: flex;
		align-items: flex-start;
		gap: var(--space-2);
		color: var(--text);
		font-size: var(--title);
		font-weight: 600;
		line-height: 1.3;
	}

	/* The whole card opens the plan; its button stays above. */
	.place::after {
		content: '';
		position: absolute;
		inset: 0;
		border-radius: var(--radius-card);
	}

	.place:focus-visible {
		outline: none;
	}

	.place:focus-visible::after {
		outline: 0.1875rem solid var(--color-focus);
		outline-offset: -0.1875rem;
	}

	.sign {
		flex: none;
		display: inline-flex;
		margin-top: 0.0625rem;
		color: var(--icon-quiet);
	}

	.name {
		min-width: 0;
		overflow-wrap: anywhere;
	}

	.group {
		margin-top: calc(-1 * var(--space-1));
		font-size: var(--footnote);
		color: var(--text-secondary);
		overflow-wrap: anywhere;
	}

	.who {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		min-width: 0;
		margin-top: auto;
		padding-top: var(--space-1);
	}

	/* The faces overlap a little, as a row of people standing together. */
	.faces {
		flex: none;
		display: flex;
		list-style: none;
	}

	.faces li {
		display: grid;
		place-items: center;
		width: 2rem;
		height: 2rem;
		border-radius: 50%;
		background: var(--fill-strong);
		color: var(--text);
		box-shadow: 0 0 0 2px var(--surface);
		font-size: var(--caption);
		font-weight: 600;
		letter-spacing: 0.02em;
	}

	.faces li + li {
		margin-left: calc(-1 * var(--space-2));
	}

	.faces li.me {
		background: var(--accent);
		color: var(--on-accent);
	}

	.faces li.more {
		background: var(--surface);
		color: var(--text-secondary);
		box-shadow:
			inset 0 0 0 1px var(--separator),
			0 0 0 2px var(--surface);
	}

	.in-words {
		min-width: 0;
		font-size: var(--footnote);
		color: var(--text-secondary);
		overflow-wrap: anywhere;
	}

	/* "I am in": an outline pill, filled while the pilot is in. */
	.in {
		position: relative;
		z-index: 1;
		align-self: flex-start;
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		min-height: var(--target);
		padding: 0 var(--space-4);
		border-radius: var(--radius-pill);
		box-shadow: inset 0 0 0 1px var(--color-border-strong);
		color: var(--link);
		font-weight: 600;
	}

	.in :global(svg) {
		font-size: 0.875rem;
	}

	.in.pressed {
		background: var(--accent);
		color: var(--on-accent);
		box-shadow: none;
	}

	.problem {
		font-size: var(--footnote);
		color: var(--danger);
	}

	@media (hover: hover) {
		.plan:has(.place:hover) {
			background: var(--fill);
		}

		.in:not(.pressed):not(:disabled):hover {
			background: var(--fill);
		}
	}
</style>
