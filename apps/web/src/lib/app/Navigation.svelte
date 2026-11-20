<!--
	The app's sections. More join as the product grows.

	On a narrow screen this is a bar of glass floating at the bottom, as on the
	iPhone. On a wide screen it is a rail down the left side. It is one list of
	links either way; only the stylesheet differs.
-->
<script lang="ts">
	import { asset } from '$app/paths';
	import { page } from '$app/state';
	import Icon, { type IconName } from '#lib/design/Icon.svelte';
	import { logbookHref, mapHref, settingsHref } from './routes';

	const sections: { name: string; icon: IconName; href: () => string; root: string }[] = [
		{ name: 'Map', icon: 'map', href: mapHref, root: '/' },
		{ name: 'Logbook', icon: 'book', href: logbookHref, root: '/logbook' },
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
	<img class="mark" src={asset('favicon.svg')} alt="Venti" width="40" height="40" />
	<ul class="glass">
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
	/* Narrow: a capsule floating over the bottom of the page. */
	nav {
		position: fixed;
		z-index: 10;
		left: 0;
		right: 0;
		bottom: calc(0.75rem + var(--safe-bottom));
		display: flex;
		justify-content: center;
		/* Only the capsule takes clicks; the map beside it stays reachable. */
		pointer-events: none;
	}

	nav.covered {
		display: none;
	}

	.mark {
		display: none;
	}

	ul {
		display: flex;
		gap: 0.125rem;
		height: var(--nav-height);
		padding: 0.25rem;
		border-radius: 999px;
		pointer-events: auto;
	}

	a {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 0.0625rem;
		min-width: 5.5rem;
		height: 100%;
		padding: 0 0.75rem;
		border-radius: 999px;
		font-size: var(--caption);
		font-weight: 600;
	}

	/* Where the pilot is: a filled shape and an accent icon. The name stays
	   in the text colour, which is readable on any ground the bar floats over. */
	a[aria-current='page'] {
		background: var(--fill-strong);
	}

	a[aria-current='page'] .icon {
		color: var(--accent);
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
			border-right: 0.5px solid var(--separator);
			pointer-events: auto;
		}

		.mark {
			display: block;
			border-radius: 0.625rem;
		}

		ul,
		ul.glass {
			flex-direction: column;
			gap: 0.375rem;
			height: auto;
			padding: 0;
			border: 0;
			border-radius: 0;
			background: none;
			box-shadow: none;
			-webkit-backdrop-filter: none;
			backdrop-filter: none;
		}

		a {
			min-width: 0;
			width: 4.5rem;
			height: 3.75rem;
			padding: 0;
			border-radius: 0.875rem;
			gap: 0.125rem;
		}
	}
</style>
