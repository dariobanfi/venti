<!--
	A token's secret, shown once: right after a program was given access. The
	server keeps no copy that can be read, so this is the pilot's one chance
	to hand it to the program.

	What stands beside it is the core's (`TokenCreated`): that it is shown
	once, what to do with it, and the words on both buttons. A button copies
	it, and it can be selected. It is kept nowhere: not in the address, not
	in the browser's storage, not in a log; the page that shows this holds
	it in memory and lets go of it when the pilot says it is copied.
-->
<script lang="ts">
	import type { TokenCreated } from '#lib/core/index.ts';
	import Icon from '#lib/design/Icon.svelte';

	interface Props {
		created: TokenCreated;
		/** The pilot says the secret is copied: the one way on. */
		ondone: () => void;
	}

	let { created, ondone }: Props = $props();

	const id = $props.id();
	/** What became of copying, in a line. */
	let copied = $state.raw<string>();
	let done = $state<HTMLButtonElement>();

	/** Brings the pilot back to the one way on, when they tried to leave another way. */
	export function hold(): void {
		done?.focus();
	}

	async function copy() {
		try {
			await navigator.clipboard.writeText(created.secret);
			copied = 'Copied.';
		} catch {
			// Browsers hand the clipboard only to pages served securely.
			copied = 'This browser did not let the page copy. Select it and copy it.';
		}
	}
</script>

<section class="group" aria-labelledby="{id}-name">
	<h2 class="group-title" id="{id}-name">{created.token.name}</h2>
	<div class="group-card">
		<p class="secret" aria-describedby="{id}-once">{created.secret}</p>
		<button class="group-row" type="button" onclick={copy}>
			{created.copy}
			<Icon name="copy" />
		</button>
	</div>
	{#if copied}
		<p class="group-note" role="status">{copied}</p>
	{/if}
	<p class="group-note once" id="{id}-once" role="alert">{created.shownOnce}</p>
	<p class="group-note">{created.howToUse}</p>
	<div class="leave">
		<button class="button-prominent" type="button" bind:this={done} onclick={ondone}>
			{created.done}
		</button>
	</div>
</section>

<style>
	/* In letters of one width, to be compared sign by sign. It can be
	   selected with one click. */
	.secret {
		padding: var(--space-4);
		font-family: ui-monospace, 'SF Mono', Menlo, Consolas, monospace;
		overflow-wrap: anywhere;
		-webkit-user-select: all;
		user-select: all;
	}

	.group-row :global(svg) {
		font-size: 0.875em;
	}

	/* The one thing not to overlook here. */
	.once {
		color: var(--text);
		font-weight: 600;
	}

	.leave {
		display: flex;
		justify-content: flex-end;
		margin-top: var(--space-5);
	}
</style>
