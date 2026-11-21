<!--
	The app's sections.

	On a narrow screen this is the design pack's tab bar along the bottom
	edge: Home, Today, Map, Logbook, Plans and Settings, the others reached
	from those. On a wide screen it is a website's bar across the top, as a
	sports data site has it: Venti's logo, the sections as words, and
	Settings and the account at the right. It is one list of links either
	way; which of them a width shows, and how, is the stylesheet's.
-->
<script lang="ts">
	import { page } from '$app/state';
	import Icon, { type IconName } from '#lib/design/Icon.svelte';
	// The logo, from the design pack, where the repository keeps it: the
	// lockup for paper and the one for the indigo slope.
	import logoOnLight from '../../../../../design/assets/logo/lockup-on-light.svg?url';
	import logoOnDark from '../../../../../design/assets/logo/lockup-on-dark.svg?url';
	import { getApp } from './app.svelte.ts';
	import {
		accountHref,
		everyoneHref,
		homeHref,
		logbookHref,
		mapHref,
		plansHref,
		rankingHref,
		settingsHref,
		todayHref
	} from './routes';

	const app = getApp();
	/** Who is signed in, for the right of the wide bar. */
	const account = $derived(app.account?.signedIn ? app.account.account : undefined);

	interface Section {
		name: string;
		icon: IconName;
		href: () => string;
		/** The route the section is: a page under it belongs to it, unless `exact`. */
		root: string;
		exact?: boolean;
		/** Which widths show it: the phone's bar has room for six. */
		on: 'both' | 'wide' | 'narrow';
	}

	const sections: Section[] = [
		// What is going on: the day, who is in the air, the best flights.
		{ name: 'Home', icon: 'mountain', href: homeHref, root: '/', exact: true, on: 'both' },
		{ name: 'Map', icon: 'map', href: () => mapHref(), root: '/map', on: 'both' },
		// What the forecast says about flying from the take-offs around.
		{ name: 'Today', icon: 'forecast', href: todayHref, root: '/today', on: 'both' },
		{
			name: 'Flights',
			icon: 'glider',
			href: () => everyoneHref(),
			root: '/logbook/everyone',
			on: 'wide'
		},
		{
			name: 'Ranking',
			icon: 'league',
			href: () => rankingHref(),
			root: '/logbook/ranking',
			on: 'wide'
		},
		{ name: 'Logbook', icon: 'track', href: logbookHref, root: '/logbook', on: 'both' },
		// Who means to fly where: the plans of the pilot's groups.
		{ name: 'Plans', icon: 'calendar', href: plansHref, root: '/plans', on: 'both' },
		{ name: 'Settings', icon: 'settings', href: settingsHref, root: '/settings', on: 'narrow' }
	];

	/** The section the current page belongs to: the one with the longest root its route is under. */
	const current = $derived.by(() => {
		const route = page.route.id ?? '/';
		return sections
			.filter((section) =>
				section.exact ? route === section.root : route.startsWith(section.root)
			)
			.toSorted((one, other) => other.root.length - one.root.length)[0]?.name;
	});

	/** Whether the page is under Settings, for the wide bar's Settings link. */
	const inSettings = $derived((page.route.id ?? '').startsWith('/settings'));

	/**
	 * A flight fills a narrow screen: the bar steps aside, as the iPhone's tab
	 * bar does, and the screen's own back button leads out.
	 */
	const covered = $derived(
		page.route.id === '/logbook/flight' || page.route.id === '/logbook/everyone/flight'
	);
</script>

<nav class:covered aria-label="Sections">
	<a class="brand" href={homeHref()}>
		<picture>
			<source srcset={logoOnDark} media="(prefers-color-scheme: dark)" />
			<img src={logoOnLight} alt="Venti" width="97" height="28" />
		</picture>
	</a>
	<ul>
		{#each sections as section (section.name)}
			<li class={section.on}>
				<a href={section.href()} aria-current={current === section.name ? 'page' : undefined}>
					<span class="icon"><Icon name={section.icon} /></span>
					<span>{section.name}</span>
				</a>
			</li>
		{/each}
	</ul>
	<div class="end">
		<a class="settings" href={settingsHref()} aria-current={inSettings ? 'page' : undefined}>
			<span class="icon"><Icon name="settings" /></span>
			<span>Settings</span>
		</a>
		<a class="account" href={accountHref()} class:here={page.route.id === '/settings/account'}>
			<span class="icon"><Icon name="person" /></span>
			<span>{account ? account.handle : 'Sign in'}</span>
		</a>
	</div>
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
	.end,
	li.wide {
		display: none;
	}

	ul {
		display: flex;
		height: var(--nav-height);
		/* Six sections share a phone's width. On a narrower screen, or with
		   larger text, they share what there is. */
		max-width: 36rem;
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
		padding: 0 0.125rem;
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

	/* Wide: a bar across the top, part of the page, as a website has it:
	   the logo, the sections as words, Settings and the account at the
	   right. It stays at the top while the page scrolls under it. */
	@media (min-width: 56rem) {
		nav,
		nav.covered {
			position: sticky;
			top: 0;
			right: auto;
			bottom: auto;
			display: flex;
			align-items: center;
			gap: var(--space-6);
			height: var(--nav-top);
			padding: var(--safe-top) calc(var(--space-6) + var(--safe-right)) 0
				calc(var(--space-6) + var(--safe-left));
			background: var(--surface);
			border-top: 0;
			border-bottom: 1px solid var(--separator);
		}

		.brand {
			display: flex;
			flex: none;
			height: auto;
			padding: 0;
			border-radius: var(--radius-s);
		}

		.brand img {
			display: block;
			height: 1.75rem;
			width: auto;
		}

		ul {
			flex: 1;
			align-items: center;
			gap: var(--space-1);
			height: 100%;
			max-width: none;
			margin: 0;
			padding: 0;
		}

		li,
		li.wide {
			display: block;
			flex: none;
		}

		li.narrow {
			display: none;
		}

		.end {
			display: flex;
			align-items: center;
			gap: var(--space-1);
		}

		a {
			flex-direction: row;
			gap: var(--space-2);
			height: 2rem;
			min-height: 2rem;
			padding: 0 var(--space-3);
			border-radius: var(--radius-s);
			font-size: var(--body);
			color: var(--text-secondary);
		}

		/* The bar is words; only Settings and the account keep their icon. */
		ul .icon {
			display: none;
		}

		.end a :global(svg) {
			width: 1.125rem;
			height: 1.125rem;
		}

		a[aria-current='page'],
		.end a.here {
			background: var(--fill);
			color: var(--text);
		}

		.account {
			border: 1px solid var(--separator);
		}
	}
</style>
