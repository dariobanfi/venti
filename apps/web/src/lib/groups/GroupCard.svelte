<!--
	One group as a card of Community (owner decision 26: "the groups'
	section is visual"): its picture, or its initials on a quiet disc, its
	name, its place and how many members, and for the pilot's own groups the
	last message and how many are unread. The whole card opens the group's
	page with its chat. A group found near the pilot has "Join" beside it
	where the pilot is not in it yet; `onjoin` does it.

	Every line is the core's: the summary, the last message in a line, the
	unread badge. What another pilot wrote is set as text.
-->
<script lang="ts">
	import { groupHref } from '#lib/app/routes.ts';
	import PilotFace from '#lib/community/PilotFace.svelte';
	import type { CommunityView } from '#lib/core/index.ts';

	interface Props {
		group: CommunityView;
		/** How far its home is, "12 km", for a group found near a place. */
		distance?: string;
		/** For a group the pilot may join: does it, and throws why not in words. */
		onjoin?: () => Promise<void>;
		/** The word on the button, the core's "Join". */
		joinWord?: string;
	}

	let { group, distance, onjoin, joinWord = 'Join' }: Props = $props();

	let joining = $state(false);
	let problem = $state<string>();

	async function join() {
		if (!onjoin || joining) return;
		joining = true;
		problem = undefined;
		try {
			await onjoin();
		} catch (error) {
			problem = error instanceof Error ? error.message : String(error);
		} finally {
			joining = false;
		}
	}
</script>

<li class="group" aria-label={group.name}>
	<PilotFace picture={group.picturePath} name={group.name} size="m" />
	<a class="name" href={groupHref(group.id)}>{group.name}</a>
	<span class="summary">
		{group.summary}{#if distance}{' · '}<span class="numeric">{distance}</span>{/if}
	</span>
	{#if group.lastMessage}
		<span class="last">{group.lastMessage}</span>
	{:else if group.description}
		<span class="last">{group.description}</span>
	{/if}
	{#if group.unreadBadge}
		<span class="unread numeric" aria-label="{group.unread} unread">{group.unreadBadge}</span>
	{/if}
	{#if onjoin}
		<button class="button-plain join" type="button" disabled={joining} onclick={join}>
			{joinWord}
		</button>
	{/if}
	{#if problem}
		<p class="problem" role="alert">{problem}</p>
	{/if}
</li>

<style>
	.group {
		position: relative;
		display: grid;
		grid-template-columns: auto minmax(0, 1fr) auto;
		grid-template-areas:
			'face name badge'
			'face summary badge'
			'last last last'
			'join join join'
			'problem problem problem';
		align-items: center;
		align-content: start;
		column-gap: var(--space-3);
		row-gap: 0.125rem;
		min-width: 0;
		padding: var(--space-3) var(--space-4) var(--space-4);
		background: var(--surface);
		border: 1px solid var(--separator);
		border-radius: var(--radius-card);
	}

	.group > :global(.face) {
		grid-area: face;
	}

	.name {
		grid-area: name;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--text);
		font-weight: 600;
	}

	/* The whole card opens the group. */
	.name::after {
		content: '';
		position: absolute;
		inset: 0;
		border-radius: inherit;
	}

	.summary {
		grid-area: summary;
		min-width: 0;
		font-size: var(--footnote);
		color: var(--text-secondary);
		overflow-wrap: anywhere;
	}

	.last {
		grid-area: last;
		margin-top: var(--space-2);
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
		font-size: var(--subheadline);
		color: var(--text-secondary);
		overflow-wrap: anywhere;
	}

	.unread {
		grid-area: badge;
		align-self: start;
		min-width: var(--space-5);
		height: var(--space-5);
		padding: 0 var(--space-2);
		border-radius: var(--radius-pill);
		background: var(--accent);
		color: var(--on-accent);
		font-size: var(--caption);
		font-weight: 600;
		line-height: var(--space-5);
		text-align: center;
	}

	.join {
		grid-area: join;
		position: relative;
		z-index: 1;
		justify-self: start;
		margin: var(--space-2) 0 0 calc(-1 * var(--space-3));
	}

	.problem {
		grid-area: problem;
		font-size: var(--footnote);
		color: var(--danger);
	}

	@media (hover: hover) {
		.group:hover {
			border-color: var(--fill-strong);
		}
	}
</style>
