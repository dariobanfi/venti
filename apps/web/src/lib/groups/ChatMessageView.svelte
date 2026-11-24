<!--
	One message of a group's chat: who wrote it and when, its text (set as
	text, never as markup, with its line breaks), its pictures, its Venti
	links as cards, and for a plan of the group the plan's card with "I am
	in". A removed message keeps its place and says who removed it. Its
	menu removes it (its author, the group's owner and admins) or reports
	it, or one of its pictures, to whoever runs the server.
-->
<script lang="ts">
	import PilotFace from '#lib/community/PilotFace.svelte';
	import type { ChatMessage } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import Menu, { type MenuItem } from '#lib/design/Menu.svelte';
	import { postedWords } from './chat';
	import { dayTitle } from './days';
	import MessageCard from './MessageCard.svelte';
	import type { GroupsModel } from './model.svelte';
	import PlanCard from './PlanCard.svelte';

	interface Props {
		message: ChatMessage;
		/** Whether the pilot removes anyone's message: the group's owner and admins. */
		canModerate: boolean;
		groups: GroupsModel | undefined;
		myName: string | undefined;
		onremove: () => void;
		/** Reports the message, or with an index the picture at it. */
		onreport: (picture?: number) => void;
		/** Told after the pilot's word on the plan went through. */
		onchanged: () => void;
	}

	let { message, canModerate, groups, myName, onremove, onreport, onchanged }: Props = $props();

	const text = format();
	const when = $derived(postedWords(message.postedAt, new Date()));

	const actions = $derived.by((): MenuItem[] => {
		if (message.removedWords) return [];
		const items: MenuItem[] = [];
		if (message.isMine || canModerate) {
			items.push({ label: 'Remove', icon: 'trash', destructive: true, onselect: onremove });
		}
		if (!message.isMine) {
			items.push({ label: 'Report this message', icon: 'warning', onselect: () => onreport() });
			message.pictures.forEach((_, index) =>
				items.push({
					label: message.pictures.length > 1 ? `Report picture ${index + 1}` : 'Report the picture',
					icon: 'warning',
					onselect: () => onreport(index)
				})
			);
		}
		return items;
	});
</script>

<li class="message" class:mine={message.isMine} aria-label="{message.authorName}, {when}">
	<PilotFace name={message.authorName} />
	<div class="body">
		<div class="head">
			<span class="author">{message.isMine ? 'You' : message.authorName}</span>
			<time class="when numeric" datetime={message.postedAt}>{when}</time>
			{#if actions.length > 0}
				<span class="menu">
					<Menu label="More about this message" items={actions} />
				</span>
			{/if}
		</div>
		{#if message.removedWords}
			<p class="removed">{message.removedWords}</p>
		{:else}
			{#if message.text}
				<p class="text">{message.text}</p>
			{/if}
			{#if message.pictures.length > 0}
				<div class="pictures" class:one={message.pictures.length === 1}>
					{#each message.pictures as path, index (path)}
						<a href={path} target="_blank" rel="noopener">
							<img
								src={path}
								alt="Picture {index + 1} of {message.pictures.length}"
								loading="lazy"
							/>
						</a>
					{/each}
				</div>
			{/if}
			{#each message.cards as card, index (`${card.kind}:${card.id}:${index}`)}
				<MessageCard {card} />
			{/each}
			{#if message.plan && groups}
				<ul class="plan">
					<PlanCard
						plan={message.plan}
						day={dayTitle(message.plan.day, groups.today, text)}
						{groups}
						{myName}
						{onchanged}
					/>
				</ul>
			{/if}
		{/if}
	</div>
</li>

<style>
	.message {
		display: flex;
		align-items: flex-start;
		gap: var(--space-3);
		padding: var(--space-2) 0;
		min-width: 0;
	}

	.body {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		min-width: 0;
	}

	.head {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		min-height: 2rem;
	}

	.author {
		font-weight: 600;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.when {
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	.menu {
		margin-left: auto;
	}

	.text {
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}

	.removed {
		font-style: italic;
		color: var(--text-secondary);
	}

	.pictures {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 10rem));
		gap: var(--space-1);
	}

	.pictures.one {
		grid-template-columns: minmax(0, 18rem);
	}

	.pictures a {
		display: block;
		border-radius: var(--radius-card);
		overflow: hidden;
		background: var(--fill);
	}

	.pictures img {
		display: block;
		width: 100%;
		aspect-ratio: 4 / 3;
		object-fit: cover;
	}

	.plan {
		list-style: none;
		max-width: 22rem;
	}
</style>
