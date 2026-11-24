<!--
	A Venti link in a message, as the server made it a card (docs/api.md,
	"Links become cards"): a flight with its pilot, distance, day and take-off
	and its small track drawn as a line; a site with its use; an open plan
	with its day and who is in, and "I'm in"; a pilot with their face; or
	what is gone. The card opens the thing. The lines are the core's.
-->
<script lang="ts">
	import { messageOf } from '#lib/core/index.ts';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { openPlanHref, pilotHref, placeHref, publicFlightHref } from '#lib/app/routes.ts';
	import PilotFace from '#lib/community/PilotFace.svelte';
	import type { CardView } from '#lib/core/index.ts';
	import Icon from '#lib/design/Icon.svelte';
	import TrackThumbnail from '#lib/logbook/TrackThumbnail.svelte';

	interface Props {
		card: CardView;
	}

	let { card }: Props = $props();

	const app = getApp();

	const href = $derived.by(() => {
		switch (card.kind) {
			case 'flight':
				return publicFlightHref(card.id);
			case 'site':
				return card.position ? placeHref(card.position, card.id) : undefined;
			case 'open_plan':
				return openPlanHref(card.id);
			case 'pilot':
				return pilotHref(card.id);
			default:
				return undefined;
		}
	});

	/** The pilot's word on an open plan, as they last gave it here. */
	let amIn = $state<boolean>();
	const isIn = $derived(amIn ?? card.amIn);
	let busy = $state(false);
	let problem = $state<string>();

	async function toggle() {
		const plans = app.openPlans;
		if (!plans || busy) return;
		busy = true;
		problem = undefined;
		try {
			const plan = await plans.setIn(card.id, !isIn);
			amIn = plan.amIn;
		} catch (error) {
			problem = messageOf(error);
		} finally {
			busy = false;
		}
	}
</script>

<div class="card {card.kind}">
	{#if card.kind === 'flight'}
		<span class="art"><TrackThumbnail outline={card.track} /></span>
	{:else if card.kind === 'pilot'}
		<span class="art"><PilotFace picture={card.picturePath} name={card.title} /></span>
	{:else if card.kind === 'site'}
		<span class="art sign"><Icon name="takeoff" /></span>
	{:else if card.kind === 'open_plan'}
		<span class="art sign"><Icon name="calendar" /></span>
	{/if}
	<span class="lines">
		{#if href}
			<a class="title" {href}>{card.title}</a>
		{:else}
			<span class="title gone">{card.title}</span>
		{/if}
		{#if card.subtitle}
			<span class="subtitle">{card.subtitle}</span>
		{/if}
	</span>
	{#if card.kind === 'open_plan' && app.account?.signedIn}
		<button class="in" type="button" aria-pressed={isIn} disabled={busy} onclick={toggle}>
			{#if isIn}<Icon name="check" />{/if}
			{app.said?.community.imIn}
		</button>
	{/if}
	{#if problem}
		<p class="problem" role="alert">{problem}</p>
	{/if}
</div>

<style>
	.card {
		position: relative;
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-3);
		min-width: 0;
		max-width: 22rem;
		padding: var(--space-2) var(--space-3);
		background: var(--surface);
		border: 1px solid var(--separator);
		border-radius: var(--radius-card);
	}

	.art {
		flex: none;
		display: inline-flex;
	}

	.sign {
		display: grid;
		place-items: center;
		width: 2rem;
		height: 2rem;
		color: var(--icon-quiet);
	}

	.lines {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
		min-width: 0;
	}

	.title {
		color: var(--text);
		font-weight: 600;
		overflow-wrap: anywhere;
	}

	.title.gone {
		color: var(--text-secondary);
		font-weight: 400;
	}

	/* The whole card opens what it is about. */
	a.title::after {
		content: '';
		position: absolute;
		inset: 0;
		border-radius: inherit;
	}

	.subtitle {
		font-size: var(--footnote);
		color: var(--text-secondary);
		overflow-wrap: anywhere;
	}

	.in {
		position: relative;
		z-index: 1;
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		min-height: var(--target);
		padding: 0 var(--space-3);
		border-radius: var(--radius-pill);
		background: var(--fill);
		color: var(--link);
		font-weight: 600;
	}

	.in[aria-pressed='true'] {
		background: var(--accent);
		color: var(--on-accent);
	}

	.problem {
		flex-basis: 100%;
		font-size: var(--footnote);
		color: var(--danger);
	}
</style>
