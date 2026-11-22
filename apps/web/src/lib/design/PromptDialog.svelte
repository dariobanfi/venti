<!--
	Asks for one line of text before something is done with it: the name of a
	group, an invite code. The iPhone has an alert with a text field for this.

	It stays open until what was typed has been taken: while that is under
	way the buttons wait, and a refusal stands under the field, where the
	pilot can change what was typed and try again.
-->
<script lang="ts">
	import { getApp } from '#lib/app/app.svelte.ts';
	import Dialog from './Dialog.svelte';

	const app = getApp();

	interface Props {
		open: boolean;
		title: string;
		/** What will happen, in a sentence or two. */
		message?: string;
		/** What the field holds: its name for a screen reader, and shown while it is empty. */
		label: string;
		/** What the field holds when the dialog opens. */
		value?: string;
		/** The words on the button that goes ahead. */
		confirm: string;
		/** Text that is not prose, such as a code: no capitals, no corrections. */
		verbatim?: boolean;
		/** Whether nothing may be typed: a line that is asked for and not needed. */
		optional?: boolean;
		/** Whether what was typed is being worked on. */
		working?: boolean;
		/** Why it was not taken, in words. */
		refusal?: string;
		onconfirm: (typed: string) => void;
		oncancel: () => void;
	}

	let {
		open,
		title,
		message,
		label,
		value = '',
		confirm,
		verbatim = false,
		optional = false,
		working = false,
		refusal,
		onconfirm,
		oncancel
	}: Props = $props();

	const id = $props.id();
	let typed = $state('');

	// A dialog that opens starts from what it was given, not from what was
	// typed the last time.
	$effect(() => {
		if (open) typed = value;
	});

	function submit(event: SubmitEvent) {
		event.preventDefault();
		onconfirm(typed);
	}
</script>

<Dialog
	{open}
	presentation="alert"
	decision
	labelledby="{id}-title"
	describedby={message ? `${id}-message` : undefined}
	onclose={oncancel}
>
	<form onsubmit={submit} novalidate>
		<div class="words">
			<h2 id="{id}-title">{title}</h2>
			{#if message}
				<p id="{id}-message">{message}</p>
			{/if}
			<!-- svelte-ignore a11y_autofocus -->
			<input
				type="text"
				autocomplete="off"
				autocapitalize={verbatim ? 'none' : undefined}
				autocorrect={verbatim ? 'off' : undefined}
				spellcheck={verbatim ? false : undefined}
				placeholder={label}
				aria-label={label}
				autofocus
				bind:value={typed}
			/>
			{#if refusal}
				<p class="refusal" role="alert">{refusal}</p>
			{/if}
		</div>
		<div class="choices">
			<button
				class="choice go"
				type="submit"
				disabled={working || (!optional && typed.trim() === '')}
			>
				{confirm}
			</button>
			<button class="choice" type="button" onclick={oncancel}>{app.said?.common.cancel}</button>
		</div>
	</form>
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

	.words p {
		margin-top: 0.375rem;
		font-size: var(--footnote);
	}

	input {
		width: 100%;
		min-height: var(--target);
		margin-top: 0.875rem;
		padding: 0 0.75rem;
		border: 0.5px solid var(--separator);
		border-radius: var(--radius-control);
		background: var(--background);
		color: var(--text);
		font: inherit;
	}

	input::placeholder {
		color: var(--text-secondary);
	}

	.words .refusal {
		margin-top: 0.625rem;
		color: var(--danger);
	}

	.choices {
		display: flex;
		flex-direction: column;
	}

	.choice {
		min-height: var(--target);
		border-top: 0.5px solid var(--separator);
		color: var(--link);
	}

	.go {
		font-weight: 600;
	}

	@media (hover: hover) {
		.choice:not(:disabled):hover {
			background: var(--fill);
		}
	}
</style>
