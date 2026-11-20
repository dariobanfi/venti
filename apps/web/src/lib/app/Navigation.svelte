<!--
	The app's sections. More join as the product grows.

	On a narrow screen this is the design pack's tab bar along the bottom
	edge. On a wide screen it is a rail down the left side. It is one list of
	links either way; only the stylesheet differs.
-->
<script lang="ts">
	import { page } from '$app/state';
	import Icon, { type IconName } from '#lib/design/Icon.svelte';
	// The app icon, from the design pack, where the repository keeps it.
	import appIcon from '../../../../../design/assets/app-icon/icon-small.svg?url';
	import { logbookHref, mapHref, plansHref, settingsHref } from './routes';

	const sections: { name: string; icon: IconName; href: () => string; root: string }[] = [
		{ name: 'Map', icon: 'map', href: mapHref, root: '/' },
		{ name: 'Logbook', icon: 'track', href: logbookHref, root: '/logbook' },
		// Who means to fly where: the plans of the pilot's groups.
		{ name: 'Plans', icon: 'calendar', href: plansHref, root: '/plans' },
		{ name: 'Settings', icon: 'settings', href: settingsHref, root: '/settings' }
	];

	/** The section the current page belongs to: the deepest one its route is under. */
	const current = $derived(
		sections.findLast((section) => (page.route.id ?? '/').startsWith(section.root))?.name
	);

	/**
	 * A flight fills a narrow screen: the bar steps aside, as the iPhone's tab
	 * bar does, and the screen's own back button leads out.
	 */
	const covered = $derived(
		page.route.id === '/logbook/flight' || page.route.id === '/logbook/everyone/flight'
	);
</script>

<nav class:covered aria-label="Sections">
	<img class="mark" src={appIcon} alt="Venti" width="40" height="40" />
	<ul>
		{#each sections as section (section.name)}
			<li>
				<a href={section.href()} aria-current={current === section.name ? 'page' : undefined}>
					<span class="icon"><Icon name={section.icon} /></span>
					<span>{section.name}</span>
				</a>
			</li>
		{/each}
	</ul>
</nav>

<style>
	/* Narrow: the pack's tab bar, along the bottom edge of the screen. */
	nav {
		position: fixed;
		z-index: 10;
		left: 0;
		right: 0;
		bottom: 0;
		padding: 0 var(--safe-right) var(--safe-bottom) var(--safe-left);
		background: var(--surface);
		border-top: 1px solid var(--separator);
	}

	nav.covered {
		display: none;
	}

	.mark {
		display: none;
	}

	ul {
		display: flex;
		height: var(--nav-height);
		/* Four sections fit a phone's width. On a narrower screen, or with
		   larger text, they share what there is. */
		max-width: 32rem;
		margin: 0 auto;
		padding: 0.25rem;
	}

	li {
		flex: 1 1 0;
		min-width: 0;
	}

	a {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 0.125rem;
		height: 100%;
		min-height: var(--target);
		padding: 0 0.25rem;
		border-radius: var(--radius-m);
		font-size: var(--caption);
		font-weight: 500;
		color: var(--icon-quiet);
	}

	a span:last-child {
		max-width: 100%;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	/* Where the pilot is: the name in the text colour, the icon in coral,
	   as the pack's tab bar has it. */
	a[aria-current='page'] {
		color: var(--text);
		font-weight: 600;
	}

	a[aria-current='page'] .icon {
		color: var(--color-accent-text);
	}

	@media (hover: hover) {
		a:not([aria-current='page']):hover {
			background: var(--fill);
		}
	}

	/* Wide: a rail down the side, part of the page rather than floating on it. */
	@media (min-width: 56rem) {
		nav,
		nav.covered {
			top: 0;
			right: auto;
			bottom: 0;
			width: var(--nav-side);
			display: flex;
			flex-direction: column;
			justify-content: flex-start;
			align-items: center;
			gap: 1.25rem;
			padding: calc(1.25rem + var(--safe-top)) 0 1.25rem;
			background: var(--surface);
			border-top: 0;
			border-right: 1px solid var(--separator);
			pointer-events: auto;
		}

		.mark {
			display: block;
			border-radius: 0.625rem;
		}

		ul {
			flex-direction: column;
			gap: 0.375rem;
			height: auto;
			margin: 0;
			padding: 0;
		}

		li {
			flex: none;
		}

		a {
			width: 4.5rem;
			height: 3.75rem;
			padding: 0;
			border-radius: var(--radius-m);
			gap: 0.125rem;
		}
	}
</style>
