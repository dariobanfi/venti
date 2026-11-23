<!--
	Asks before something that cannot be undone. The safe answer has the focus,
	so Enter and Escape both leave things as they are.
-->
<script lang="ts">
	import { getApp } from '#lib/app/app.svelte.ts';
	import Dialog from './Dialog.svelte';

	const app = getApp();

	interface Props {
		open: boolean;
		title: string;
		/** What will happen, in a sentence or two; none where the title says it all. */
		message?: string;
		/** The words on the button that goes ahead. */
		confirm: string;
		/** The words on the button that leaves things as they are, where the core has its own. */
		cancel?: string;
		onconfirm: () => void;
		oncancel: () => void;
		/** Whether going ahead cannot be undone: its button is then the danger colour. */
		destructive?: boolean;
	}

	let {
		open,
		title,
		message,
		confirm,
		cancel,
		onconfirm,
		oncancel,
		destructive = true
	}: Props = $props();

	/** Two of these in one page name their own words. */
	const id = $props.id();
</script>

<Dialog
	{open}
	presentation="alert"
	decision
	labelledby="{id}-title"
	describedby={message ? `${id}-message` : undefined}
	onclose={oncancel}
>
	<div class="words">
		<h2 id="{id}-title">{title}</h2>
		{#if message}
			<p id="{id}-message">{message}</p>
		{/if}
	</div>
	<div class="choices">
		<button class="choice" class:destructive type="button" onclick={onconfirm}>{confirm}</button>
		<!-- svelte-ignore a11y_autofocus -->
		<button class="choice cancel" type="button" autofocus onclick={oncancel}
			>{cancel ?? app.said?.common.cancel}</button
		>
	</div>
</Dialog>

<style>
	.words {
		padding: 1.25rem 1.25rem 1rem;
		text-align: center;
	}

	h2 {
		font-size: var(--body);
		font-weight: 600;
	}

	p {
		margin-top: 0.375rem;
		font-size: var(--footnote);
	}

	.choices {
		display: flex;
		flex-direction: column;
	}

	.choice {
		min-height: var(--target);
		border-top: 0.5px solid var(--separator);
	}

	.destructive {
		color: var(--danger);
	}

	.cancel {
		color: var(--link);
		font-weight: 600;
	}

	@media (hover: hover) {
		.choice:hover {
			background: var(--fill);
		}
	}
</style>
