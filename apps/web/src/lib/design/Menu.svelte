<!--
	A button that opens a short list of things to do, for the actions a screen
	does not need on show all the time. The iPhone app has the system's menu
	for this.

	The list is the browser's own popover: a click beside it or Escape closes
	it, and focus goes back to the button. The arrow keys move through it.
-->
<script lang="ts" module>
	import type { IconName } from './Icon.svelte';

	export interface MenuItem {
		label: string;
		icon: IconName;
		/** Something that cannot be undone, shown in red. */
		destructive?: boolean;
		onselect: () => void;
	}
</script>

<script lang="ts">
	import Icon from './Icon.svelte';

	interface Props {
		/** What the button is called: it shows only three dots. */
		label: string;
		items: MenuItem[];
		/** A round glass button, for a menu that floats over a map. */
		floating?: boolean;
	}

	let { label, items, floating = false }: Props = $props();

	const id = $props.id();
	let button: HTMLButtonElement;
	let menu: HTMLDivElement;

	const entries = () => [...menu.querySelectorAll('button')];

	/** Puts the list under its button, their right edges in line, before it shows. */
	function opening(event: ToggleEvent) {
		if (event.newState !== 'open') return;
		const place = button.getBoundingClientRect();
		menu.style.top = `${place.bottom + 6}px`;
		menu.style.right = `${document.documentElement.clientWidth - place.right}px`;
	}

	/** The keyboard starts on the first entry, unless it is in the list already. */
	function opened(event: ToggleEvent) {
		if (event.newState === 'open' && !menu.contains(document.activeElement)) entries()[0]?.focus();
	}

	/** On the button, the arrow keys open the list at its first or last entry. */
	function reached(event: KeyboardEvent) {
		if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
		event.preventDefault();
		if (!menu.matches(':popover-open')) menu.showPopover();
		entries()
			.at(event.key === 'ArrowDown' ? 0 : -1)
			?.focus();
	}

	function keyed(event: KeyboardEvent) {
		const all = entries();
		const at = all.indexOf(document.activeElement as HTMLButtonElement);
		const to: Record<string, number> = {
			ArrowDown: (at + 1) % all.length,
			ArrowUp: (at - 1 + all.length) % all.length,
			Home: 0,
			End: all.length - 1
		};
		if (event.key in to) {
			event.preventDefault();
			all[to[event.key]].focus();
		} else if (event.key === 'Tab') {
			// Leaving the list with Tab closes it, as menus do.
			menu.hidePopover();
		}
	}

	function chose(item: MenuItem) {
		menu.hidePopover();
		item.onselect();
	}
</script>

<button
	bind:this={button}
	class="opener"
	class:glass={floating}
	class:floating
	type="button"
	popovertarget={id}
	aria-haspopup="menu"
	aria-label={label}
	title={label}
	onkeydown={reached}
>
	<Icon name="more" />
</button>

<div
	{id}
	bind:this={menu}
	class="menu"
	popover
	role="menu"
	tabindex="-1"
	aria-label={label}
	onbeforetoggle={opening}
	ontoggle={opened}
	onkeydown={keyed}
>
	{#each items as item (item.label)}
		<button
			type="button"
			role="menuitem"
			class:destructive={item.destructive}
			onclick={() => chose(item)}
		>
			{item.label}
			<Icon name={item.icon} />
		</button>
	{/each}
</div>

<style>
	.opener {
		display: grid;
		place-items: center;
		width: var(--target);
		height: var(--target);
		border-radius: 50%;
		color: var(--link);
		pointer-events: auto;
	}

	.floating {
		width: 3rem;
		height: 3rem;
		color: var(--text);
	}

	@media (hover: hover) {
		.opener:not(.floating):hover {
			background: var(--fill);
		}

		.floating:hover {
			background: var(--surface);
		}
	}

	.menu {
		/* The browser would put a popover in the middle of the window. */
		position: fixed;
		inset: auto;
		margin: 0;
		min-width: 15rem;
		padding: 0;
		border: 0;
		border-radius: var(--radius-card);
		background: var(--surface);
		color: var(--text);
		box-shadow: var(--shadow-float);
		overflow: hidden;
		/* The bar a floating menu sits in lets clicks through to the map. */
		pointer-events: auto;
	}

	.menu:popover-open {
		animation: unfold 0.14s ease-out;
		transform-origin: top right;
	}

	.menu button {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1.5rem;
		width: 100%;
		min-height: var(--target);
		padding: 0.5rem 1rem;
		text-align: left;
	}

	.menu button + button {
		border-top: 0.5px solid var(--separator);
	}

	.menu button:focus-visible {
		outline-offset: -0.1875rem;
		border-radius: 0.75rem;
	}

	.destructive {
		color: var(--danger);
	}

	@media (hover: hover) {
		.menu button:hover {
			background: var(--fill);
		}
	}

	@keyframes unfold {
		from {
			opacity: 0;
			transform: scale(0.9);
		}
	}
</style>
