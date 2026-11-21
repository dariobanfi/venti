<!--
	A round glass button that floats over a map: one of the few places the app
	draws a control of its own. Its label is for screen readers and appears as a
	tooltip under a mouse.
-->
<script lang="ts">
	import Icon, { type IconName } from '#lib/design/Icon.svelte';

	interface Props {
		label: string;
		icon: IconName;
		/** For a button that switches something on and off: whether it is on. */
		pressed?: boolean;
		/** The link this leads to, for a button that goes somewhere. */
		href?: string;
		onclick?: () => void;
	}

	let { label, icon, pressed, href, onclick }: Props = $props();
</script>

{#if href}
	<a class="glass button" {href} aria-label={label} title={label}>
		<Icon name={icon} />
	</a>
{:else}
	<button
		class="glass button"
		class:pressed
		type="button"
		aria-label={label}
		aria-pressed={pressed}
		title={label}
		{onclick}
	>
		<Icon name={icon} filled={pressed} />
	</button>
{/if}

<style>
	.button {
		display: grid;
		place-items: center;
		width: var(--map-button, 3rem);
		height: var(--map-button, 3rem);
		border-radius: 50%;
		color: var(--text);
		/* Clicks go to the button, not through the row it sits in. */
		pointer-events: auto;
	}

	.pressed {
		color: var(--link);
	}

	/* A wide screen's controls are compact, about 32 px: a mouse needs no
	   finger's target. */
	@media (min-width: 56rem) {
		.button {
			--map-button: 2.25rem;
		}

		.button :global(svg) {
			width: 1.125rem;
			height: 1.125rem;
		}
	}

	@media (hover: hover) {
		.button:hover {
			background: var(--surface);
		}
	}

	.button:active {
		transform: scale(0.94);
	}
</style>
