<!--
	Something chosen on the map or from a list beside it: a wind station, a
	flying site.

	On a narrow screen it is a sheet, as on the iPhone. A wide screen has room
	to keep the map in reach: it is a card at the map's side, and choosing
	another arrow or sign on the map fills the same card.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';
	import Icon from '#lib/design/Icon.svelte';
	import Sheet from '#lib/design/Sheet.svelte';

	interface Props {
		/** What the panel shows, by its identifier. Nothing closes the panel. */
		shown: string | undefined;
		title: string;
		/** Whether the screen is wide enough for the card beside the map. */
		wide: boolean;
		onclose: () => void;
		children: Snippet;
	}

	let { shown, title, wide, onclose, children }: Props = $props();

	const id = $props.id();
	let heading = $state<HTMLHeadingElement>();

	// The card is not a dialog and does not take the keyboard by itself.
	// Whoever chose something from a list is taken to it.
	$effect(() => {
		if (shown) heading?.focus();
	});

	function keyed(event: KeyboardEvent) {
		// Not when something inside took the key for itself, as a chart does
		// to let go of a reading.
		if (event.key === 'Escape' && !event.defaultPrevented) onclose();
	}
</script>

{#if wide}
	{#if shown}
		<!-- Escape closes the card from anywhere inside it; the button does the same. -->
		<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
		<aside class="card" aria-labelledby="{id}-title" onkeydown={keyed}>
			<header>
				<h2 id="{id}-title" tabindex="-1" bind:this={heading}>{title}</h2>
				<button type="button" aria-label="Close" title="Close" onclick={onclose}>
					<Icon name="close" />
				</button>
			</header>
			<div class="body">
				{@render children()}
			</div>
		</aside>
	{/if}
{:else}
	<Sheet open={Boolean(shown)} {title} {onclose}>
		{#if shown}
			{@render children()}
		{/if}
	</Sheet>
{/if}

<style>
	.card {
		display: flex;
		flex-direction: column;
		width: 24rem;
		max-height: 100%;
		border-radius: var(--radius-card);
		background: var(--background);
		box-shadow: var(--shadow-float);
		overflow: hidden;
		pointer-events: auto;
	}

	header {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		min-height: 3.5rem;
		padding: 0 0.5rem 0 1.25rem;
	}

	h2 {
		flex: 1;
		min-width: 0;
		font-size: var(--body);
		font-weight: 600;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		/* Focus is put here by the app, not reached with Tab. */
		outline: none;
	}

	button {
		flex: none;
		display: grid;
		place-items: center;
		width: var(--target);
		height: var(--target);
		border-radius: 50%;
		color: var(--text-secondary);
	}

	@media (hover: hover) {
		button:hover {
			background: var(--fill);
		}
	}

	.body {
		overflow-y: auto;
		padding: 0.25rem 1rem 1.25rem;
	}
</style>
