<!--
	One plan in a line of the board: where, who is in, in which group, and at
	the end the time if it has one and a tick where the pilot is in. The whole
	row leads to the plan. Who is in is the core's sentence.

	What another pilot wrote, a place or a group's name, is set as text.
-->
<script lang="ts">
	import { planHref } from '#lib/app/routes.ts';
	import type { PlanView } from '#lib/core/index.ts';
	import Icon from '#lib/design/Icon.svelte';

	let { plan }: { plan: PlanView } = $props();
</script>

<a class="plan" href={planHref(plan.id)}>
	<span class="what">
		<span class="place">{plan.place}</span>
		<span class="lesser">{plan.inWords}</span>
		{#if plan.groupName}
			<span class="lesser">{plan.groupName}</span>
		{/if}
	</span>
	<span class="when">
		{#if plan.meetAt}
			<span class="time numeric">{plan.meetAt}</span>
		{/if}
		{#if plan.amIn}
			<span class="tick" role="img" aria-label="You are in"><Icon name="verified" /></span>
		{/if}
	</span>
</a>

<style>
	.plan {
		display: flex;
		align-items: flex-start;
		gap: 0.75rem;
		min-height: var(--target);
		padding: 0.625rem 1rem;
	}

	.plan:focus-visible {
		outline-offset: -0.1875rem;
		border-radius: 0.75rem;
	}

	.what {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 0.1875rem;
		min-width: 0;
	}

	.place {
		font-weight: 600;
		overflow-wrap: anywhere;
	}

	.lesser {
		font-size: var(--footnote);
		color: var(--text-secondary);
		overflow-wrap: anywhere;
	}

	.when {
		flex: none;
		display: flex;
		flex-direction: column;
		align-items: flex-end;
		gap: 0.1875rem;
	}

	.time {
		font-weight: 600;
	}

	.tick {
		color: var(--link);
		font-size: 0.875rem;
	}

	@media (hover: hover) {
		.plan:hover {
			background: var(--fill);
		}
	}
</style>
