<!--
	One open plan as a card of Community, in the manner of a group's plan
	(`groups/PlanCard.svelte`): its day and time at the top, the place with
	the pack's site sign, the author's face and name, what the author offers
	in the core's words ("2 seats free", "Looking for company"), the note,
	the faces of who is in with the core's sentence, and "I'm in", a button
	that stays pressed while the pilot is in. Signed out, "I'm in" leads to
	the account. The place leads to the plan's page.

	An open plan names its pilots by name alone, with no picture: the faces
	are initials. What another pilot wrote is set as text.
-->
<script lang="ts">
	import { messageOf } from '#lib/core/index.ts';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { accountHref, openPlanHref } from '#lib/app/routes.ts';
	import type { OpenPlanView } from '#lib/core/index.ts';
	import Icon from '#lib/design/Icon.svelte';
	import { facesOf } from '#lib/groups/faces.ts';
	import type { OpenPlansModel } from './open-plans.svelte';
	import PilotFace from './PilotFace.svelte';

	interface Props {
		plan: OpenPlanView;
		/** What the day is called: "Today", "Tomorrow", "Fri 21 Nov". */
		day: string;
		model: OpenPlansModel;
		/** The name the pilot of this browser is shown by, for their face. */
		myName: string | undefined;
	}

	let { plan, day, model, myName }: Props = $props();

	const app = getApp();

	const shown = $derived(facesOf(plan.pilotsIn, plan.amIn, myName));
	let busy = $state(false);
	/** Why the pilot's word did not go through, in words. */
	let problem = $state<string>();

	async function toggle() {
		busy = true;
		problem = undefined;
		try {
			await model.setIn(plan.id, !plan.amIn);
		} catch (error) {
			problem = messageOf(error);
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
	<a class="place" href={openPlanHref(plan.id)}>
		<span class="sign"><Icon name="pin" /></span>
		<span class="name">{plan.place}</span>
	</a>
	<div class="author">
		<PilotFace name={plan.author} />
		<span class="lines">
			<span class="by">{plan.isMine ? 'You' : plan.author}</span>
			<span class="offer">{plan.offerWords}</span>
		</span>
	</div>
	{#if plan.note}
		<p class="note">{plan.note}</p>
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
	{#if model.signedIn}
		<button
			class="in"
			class:pressed={plan.amIn}
			type="button"
			aria-pressed={plan.amIn}
			disabled={busy}
			onclick={toggle}
		>
			{#if plan.amIn}<Icon name="check" />{/if}
			<span>{app.said?.community.imIn}</span>
		</button>
	{:else}
		<!-- Saying so needs an account: the way to one. -->
		<a class="in" href={accountHref()}>{app.said?.community.imIn}</a>
	{/if}
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

	.author {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		min-width: 0;
	}

	.lines {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}

	.by {
		font-size: var(--footnote);
		color: var(--text-secondary);
		overflow-wrap: anywhere;
	}

	.offer {
		font-size: var(--subheadline);
		font-weight: 600;
		color: var(--text);
	}

	.note {
		font-size: var(--footnote);
		color: var(--text-secondary);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		display: -webkit-box;
		-webkit-box-orient: vertical;
		-webkit-line-clamp: 3;
		line-clamp: 3;
		overflow: hidden;
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

	/* "I'm in": an outline pill, filled while the pilot is in. */
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
