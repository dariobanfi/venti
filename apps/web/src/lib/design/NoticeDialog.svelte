<!--
	A message for the pilot with one way out: what could not be imported, or
	why the position is not shown.
-->
<script lang="ts">
	import type { Notice } from '#lib/logbook/import-report.ts';
	import Dialog from './Dialog.svelte';

	interface Props {
		/** The message, or nothing while there is none to show. */
		notice: Notice | undefined;
		onclose: () => void;
	}

	let { notice, onclose }: Props = $props();

	// The words stay in place while the dialog closes, so it does not empty
	// before the eye has let go of it.
	let shown = $state.raw<Notice>();
	$effect(() => {
		if (notice) shown = notice;
	});
</script>

<Dialog
	open={notice !== undefined}
	presentation="alert"
	labelledby="notice-title"
	describedby="notice-text"
	{onclose}
>
	<div class="words">
		<h2 id="notice-title">{shown?.title}</h2>
		<div id="notice-text">
			{#each shown?.lines ?? [] as line (line)}
				<p>{line}</p>
			{/each}
		</div>
	</div>
	<form method="dialog">
		<!-- svelte-ignore a11y_autofocus -->
		<button class="choice" autofocus>OK</button>
	</form>
</Dialog>

<style>
	.words {
		padding: 1.25rem 1.25rem 1rem;
		text-align: center;
		overflow-y: auto;
	}

	h2 {
		font-size: var(--body);
		font-weight: 600;
	}

	p {
		margin-top: 0.375rem;
		font-size: var(--footnote);
		/* File names have no spaces to break at. */
		overflow-wrap: anywhere;
	}

	.choice {
		width: 100%;
		min-height: var(--target);
		border-top: 0.5px solid var(--separator);
		color: var(--accent);
		font-weight: 600;
	}

	@media (hover: hover) {
		.choice:hover {
			background: var(--fill);
		}
	}
</style>
