<!--
	Something shown over the page that has to be dealt with before going on.

	It is the browser's own dialog element, so the keyboard stays inside it,
	Escape closes it, focus returns to where it was, and a screen reader is told
	that the rest of the page is out of reach.

	An "alert" is a small card in the middle of the screen. A "sheet" rises from
	the bottom edge of a narrow screen, as on the iPhone, and is a card in the
	middle of a wide one.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		open: boolean;
		presentation: 'alert' | 'sheet';
		/** The id of the element that names the dialog. */
		labelledby: string;
		/** The id of the element that says what it is about, if there is one. */
		describedby?: string;
		/** An alert that asks for a decision, rather than one that only informs. */
		decision?: boolean;
		onclose: () => void;
		children: Snippet;
	}

	let {
		open,
		presentation,
		labelledby,
		describedby,
		decision = false,
		onclose,
		children
	}: Props = $props();

	let dialog: HTMLDialogElement;

	$effect(() => {
		if (open && !dialog.open) dialog.showModal();
		else if (!open && dialog.open) dialog.close();
	});

	/** A click beside the dialog closes it, unless it is waiting for a decision. */
	function clickedBeside(event: MouseEvent) {
		if (event.target === dialog && !decision) dialog.close();
	}
</script>

<!-- The click handler is for the backdrop only; every control inside has its own. -->
<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<dialog
	bind:this={dialog}
	class={presentation}
	role={decision ? 'alertdialog' : undefined}
	aria-labelledby={labelledby}
	aria-describedby={describedby}
	{onclose}
	onclick={clickedBeside}
>
	<div class="content">
		{@render children()}
	</div>
</dialog>

<style>
	dialog {
		border-radius: var(--radius-card);
		box-shadow: var(--shadow-float);
		overflow: hidden;
	}

	.content {
		display: flex;
		flex-direction: column;
		max-height: inherit;
	}

	.alert {
		width: min(20rem, calc(100vw - 2.5rem));
	}

	.sheet {
		width: min(30rem, calc(100vw - 2.5rem));
		max-height: min(46rem, calc(100dvh - 2.5rem));
		background: var(--background);
	}

	/* On a narrow screen a sheet is attached to the bottom edge. */
	@media (max-width: 55.99rem) {
		.sheet {
			width: 100%;
			max-width: 100%;
			max-height: calc(100dvh - 3rem - var(--safe-top));
			margin: auto 0 0;
			border-radius: var(--radius-panel) var(--radius-panel) 0 0;
			padding-bottom: var(--safe-bottom);
		}
	}

	dialog[open] {
		animation: appear 0.18s ease-out;
	}

	.sheet[open] {
		animation: rise 0.26s cubic-bezier(0.2, 0.8, 0.2, 1);
	}

	@media (min-width: 56rem) {
		.sheet[open] {
			animation: appear 0.18s ease-out;
		}
	}

	@keyframes appear {
		from {
			opacity: 0;
			transform: scale(0.96);
		}
	}

	@keyframes rise {
		from {
			transform: translateY(100%);
		}
	}
</style>
