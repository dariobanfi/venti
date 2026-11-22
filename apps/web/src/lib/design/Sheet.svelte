<!--
	A sheet with a title and a "Done" button over a body that scrolls: the
	iPhone's sheet with its navigation bar. It rises from the bottom edge of a
	narrow screen and is a card in the middle of a wide one.
-->
<script lang="ts">
	import { getApp } from '#lib/app/app.svelte.ts';
	import type { Snippet } from 'svelte';
	import Dialog from './Dialog.svelte';

	const app = getApp();

	interface Props {
		open: boolean;
		title: string;
		/** Leaves the upper half of a narrow screen free: see `Dialog.svelte`. */
		half?: boolean;
		onclose: () => void;
		children: Snippet;
	}

	let { open, title, half = false, onclose, children }: Props = $props();

	const id = $props.id();
</script>

<Dialog {open} presentation="sheet" {half} labelledby="{id}-title" {onclose}>
	<header>
		<h2 id="{id}-title">{title}</h2>
		<button type="button" onclick={onclose}>{app.said?.common.done}</button>
	</header>
	<div class="body">
		{@render children()}
	</div>
</Dialog>

<style>
	header {
		position: relative;
		display: flex;
		align-items: center;
		justify-content: center;
		min-height: 3.5rem;
		/* The title stays clear of the button beside it. */
		padding: 0 4.5rem;
	}

	h2 {
		font-size: var(--body);
		font-weight: 600;
		/* A long title takes a second line and is never cut: on a narrow
		   phone with large text "I took off here and it was good" was. */
		padding-block: 0.5rem;
		text-align: center;
		text-wrap: balance;
	}

	button {
		position: absolute;
		right: 0.5rem;
		min-height: var(--target);
		padding: 0 0.75rem;
		border-radius: var(--radius-control);
		color: var(--link);
		font-weight: 600;
	}

	.body {
		overflow-y: auto;
		padding: 0.5rem 1rem 1.5rem;
	}

	@media (hover: hover) {
		button:hover {
			background: var(--fill);
		}
	}
</style>
