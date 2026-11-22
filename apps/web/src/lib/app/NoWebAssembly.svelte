<!--
	Over the whole window, in a browser that has WebAssembly switched off:
	the core cannot run there, and nearly everything relies on it, so no
	screen is drawn and nothing behind this can be used. It is the browser's
	own modal dialog, which makes the rest of the page inert, and it cannot
	be closed: Escape and a close request are refused, and should a browser
	close it all the same, it opens again. Drawn as the screens draw a state
	with nothing on it.
-->
<script lang="ts">
	import Icon from '#lib/design/Icon.svelte';
	import { NO_WEBASSEMBLY } from './words';

	let dialog: HTMLDialogElement;

	$effect(() => {
		if (!dialog.open) dialog.showModal();
	});
</script>

<svelte:head>
	<title>Venti</title>
</svelte:head>

<dialog
	bind:this={dialog}
	closedby="none"
	aria-labelledby="no-webassembly-title"
	aria-describedby="no-webassembly-why"
	oncancel={(event) => event.preventDefault()}
	onclose={() => dialog.showModal()}
>
	<div class="unavailable">
		<span class="symbol"><Icon name="warning" /></span>
		<h1 id="no-webassembly-title">{NO_WEBASSEMBLY.title}</h1>
		<p id="no-webassembly-why">{NO_WEBASSEMBLY.why}</p>
		<ul class="ways">
			{#each NO_WEBASSEMBLY.ways as way (way)}
				<li>{way}</li>
			{/each}
		</ul>
	</div>
</dialog>

<style>
	/* The whole window, the navigation's place included. */
	dialog {
		position: fixed;
		inset: 0;
		width: 100%;
		height: 100%;
		margin: 0;
		background: var(--background);
		overflow: auto;
	}

	dialog:focus-visible {
		outline: none;
	}

	dialog::backdrop {
		background: var(--background);
	}

	.unavailable {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: var(--space-2);
		min-height: 100%;
		max-width: 26rem;
		margin: 0 auto;
		padding: calc(var(--space-6) + var(--safe-top)) calc(var(--gutter) + var(--safe-right))
			calc(var(--space-6) + var(--safe-bottom)) calc(var(--gutter) + var(--safe-left));
		text-align: center;
	}

	.symbol {
		color: var(--text-secondary);
		font-size: 2.75rem;
		margin-bottom: var(--space-2);
	}

	h1 {
		font-size: var(--large-title);
		font-weight: 600;
	}

	p {
		color: var(--text-secondary);
	}

	.ways {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		margin-top: var(--space-3);
		padding: 0;
		list-style: none;
	}
</style>
