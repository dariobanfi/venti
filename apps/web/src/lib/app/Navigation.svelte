<!--
	The app's sections. More join as the product grows.

	On a narrow screen this is the design pack's tab bar along the bottom
	edge. On a wide screen it is a slim column down the left side, part of
	the page: Venti's mark and name, the sections as compact rows with the
	icon beside the word, and the account at its foot, as a website's
	navigation has it. It is one list of links either way; only the
	stylesheet differs, and the account's line is the wide layout's alone,
	the narrow one reaching it under Settings as the iPhone does.
-->
<script lang="ts">
	import { page } from '$app/state';
	import Icon, { type IconName } from '#lib/design/Icon.svelte';
	// The app icon, from the design pack, where the repository keeps it.
	import appIcon from '../../../../../design/assets/app-icon/icon-small.svg?url';
	import { getApp } from './app.svelte.ts';
	import { accountHref, logbookHref, mapHref, plansHref, settingsHref, todayHref } from './routes';

	const app = getApp();
	/** Who is signed in, for the foot of the wide column. */
	const account = $derived(app.account?.signedIn ? app.account.account : undefined);

	const sections: { name: string; icon: IconName; href: () => string; root: string }[] = [
		// What the forecast says about flying from the take-offs around.
		{ name: 'Today', icon: 'forecast', href: todayHref, root: '/today' },
		{ name: 'Map', icon: 'map', href: mapHref, root: '/' },
		{ name: 'Logbook', icon: 'track', href: logbookHref, root: '/logbook' },
		// Who means to fly where: the plans of the pilot's groups.
		{ name: 'Plans', icon: 'calendar', href: plansHref, root: '/plans' },
		{ name: 'Settings', icon: 'settings', href: settingsHref, root: '/settings' }
	];

	/** The section the current page belongs to: the one with the longest root its route is under. */
	const current = $derived(
		sections
			.filter((section) => (page.route.id ?? '/').startsWith(section.root))
			.toSorted((one, other) => other.root.length - one.root.length)[0]?.name
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
	<a class="brand" href={mapHref()}>
		<img src={appIcon} alt="" width="28" height="28" />
		<span>Venti</span>
	</a>
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
	<a class="account" href={accountHref()} class:here={page.route.id === '/settings/account'}>
		<span class="icon"><Icon name="person" /></span>
		<span>{account ? account.handle : 'Sign in'}</span>
	</a>
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

	.brand,
	.account {
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

	/* Wide: a slim column down the side, part of the page rather than
	   floating on it, with compact rows: the icon beside the word. */
	@media (min-width: 56rem) {
		nav,
		nav.covered {
			top: 0;
			right: auto;
			bottom: 0;
			width: var(--nav-side);
			display: flex;
			flex-direction: column;
			gap: var(--space-4);
			padding: calc(var(--space-4) + var(--safe-top)) var(--space-2) var(--space-3);
			background: var(--surface);
			border-top: 0;
			border-right: 1px solid var(--separator);
		}

		.brand {
			display: flex;
			flex-direction: row;
			justify-content: flex-start;
			gap: var(--space-2);
			height: auto;
			min-height: 0;
			padding: 0 var(--space-2);
			font-size: var(--body);
			font-weight: 700;
			color: var(--text);
		}

		.brand img {
			border-radius: var(--radius-s);
		}

		ul {
			flex-direction: column;
			gap: 0.125rem;
			height: auto;
			max-width: none;
			margin: 0;
			padding: 0;
		}

		li {
			flex: none;
		}

		a {
			flex-direction: row;
			justify-content: flex-start;
			gap: var(--space-2);
			height: 2rem;
			min-height: 2rem;
			padding: 0 var(--space-2);
			border-radius: var(--radius-s);
			font-size: var(--subheadline);
		}

		a :global(svg) {
			width: 1.125rem;
			height: 1.125rem;
		}

		a[aria-current='page'] {
			background: var(--fill);
		}

		.account {
			display: flex;
			margin-top: auto;
		}

		/* The account is under Settings, which is the section marked; its
		   own line is marked too, without saying so twice to a reader. */
		.account.here {
			background: var(--fill);
			color: var(--text);
		}
	}
</style>
