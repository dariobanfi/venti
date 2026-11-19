<!--
	Asks before something that cannot be undone. The safe answer has the focus,
	so Enter and Escape both leave things as they are.
-->
<script lang="ts">
	import Dialog from './Dialog.svelte';

	interface Props {
		open: boolean;
		title: string;
		/** What will happen, in a sentence or two. */
		message: string;
		/** The words on the button that goes ahead. */
		confirm: string;
		onconfirm: () => void;
		oncancel: () => void;
	}

	let { open, title, message, confirm, onconfirm, oncancel }: Props = $props();
</script>

<Dialog
	{open}
	presentation="alert"
	decision
	labelledby="confirm-title"
	describedby="confirm-message"
	onclose={oncancel}
>
	<div class="words">
		<h2 id="confirm-title">{title}</h2>
		<p id="confirm-message">{message}</p>
	</div>
	<div class="choices">
		<button class="choice destructive" type="button" onclick={onconfirm}>{confirm}</button>
		<!-- svelte-ignore a11y_autofocus -->
		<button class="choice cancel" type="button" autofocus onclick={oncancel}>Cancel</button>
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
		color: var(--accent);
		font-weight: 600;
	}

	@media (hover: hover) {
		.choice:hover {
			background: var(--fill);
		}
	}
</style>
