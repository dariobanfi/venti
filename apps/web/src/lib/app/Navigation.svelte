<!--
	The app's sections.

	On a narrow screen this is the design pack's tab bar along the bottom
	edge: Home (where Today is), Map, Flights, Ranking, Community and
	Settings, the others reached from those. On a wide screen it is a website's bar across the top, as a
	sports data site has it: Venti's logo, the sections as words, and
	Settings and the account at the right. The pilot's own flights are the
	account's (owner decision 20): reached from the account, and from Flights
	by its "Mine", so they count as Flights here. It is one list of links either
	way; which of them a width shows, and how, is the stylesheet's.
-->
<script lang="ts">
	import { page } from '$app/state';
	import PilotFace from '#lib/community/PilotFace.svelte';
	import Icon, { type IconName } from '#lib/design/Icon.svelte';
	// The logo, from the design pack, where the repository keeps it: the
	// lockup for paper and the one for the indigo slope.
	import logoOnLight from '../../../../../design/assets/logo/lockup-on-light.svg?url';
	import logoOnDark from '../../../../../design/assets/logo/lockup-on-dark.svg?url';
	import { getApp } from './app.svelte.ts';
	import {
		accountHref,
		communityHref,
		everyoneHref,
		homeHref,
		mapHref,
		rankingHref,
		settingsHref
	} from './routes';

	const app = getApp();
	/** Who is signed in, for the right of the wide bar. */
	const account = $derived(app.account?.signedIn ? app.account.account : undefined);
	/**
	 * What is new for the pilot, the server's count in the core's words:
	 * a bubble on their picture, and on Settings in the phone's bar, which
	 * has no picture. Nothing signed out.
	 */
	const unread = $derived(account ? app.unread : undefined);

	interface Section {
		name: string;
		icon: IconName;
		href: () => string;
		/** The route the section is: a page under it belongs to it, unless `exact`. */
		root: string;
		/** Other routes whose pages belong to it. */
		also?: string[];
		exact?: boolean;
		/** Which widths show it: the phone's bar has room for six. */
		on: 'both' | 'wide' | 'narrow';
		/**
		 * The name with a soft hyphen where it may break, for the phone's bar:
		 * at 360 px with text at 130 % "Settings" is wider than its sixth of
		 * the bar, and takes two lines under the icon.
		 */
		breaks?: string;
	}

	const sections: Section[] = [
		// What is going on: Today's take-offs first, who is in the air, the best flights.
		{ name: 'Home', icon: 'mountain', href: homeHref, root: '/', exact: true, on: 'both' },
		{ name: 'Map', icon: 'map', href: () => mapHref(), root: '/map', on: 'both' },
		// Everyone's flights, with the pilot's own a choice at their head.
		{
			name: 'Flights',
			icon: 'track',
			href: () => everyoneHref(),
			root: '/flights',
			also: ['/settings/account/flights', '/pilots', '/places'],
			on: 'both'
		},
		{
			name: 'Ranking',
			icon: 'league',
			href: () => rankingHref(),
			root: '/ranking',
			on: 'both'
		},
		// Pilots among each other: the plans of the pilot's groups, the
		// flights of those they follow, finding a pilot, what they were told.
		// A plan's and a group's pages are still under `/plans`.
		{
			name: 'Community',
			breaks: 'Com\u00admunity',
			icon: 'people',
			href: communityHref,
			root: '/community',
			also: ['/plans'],
			on: 'both'
		},
		{
			name: 'Settings',
			breaks: 'Set\u00adtings',
			icon: 'settings',
			href: settingsHref,
			root: '/settings',
			on: 'narrow'
		}
	];

	/** The section the current page belongs to: the one with the longest root its route is under. */
	const current = $derived.by(() => {
		const route = page.route.id ?? '/';
		const under = (root: string) => route.startsWith(root);
		return sections
			.flatMap((section) =>
				[section.root, ...(section.also ?? [])]
					.filter((root) => (section.exact ? route === root : under(root)))
					.map((root) => ({ name: section.name, root }))
			)
			.toSorted((one, other) => other.root.length - one.root.length)[0]?.name;
	});

	/** Whether the page is under Settings, for the wide bar's Settings link: the pilot's flights are not. */
	const inSettings = $derived(
		(page.route.id ?? '').startsWith('/settings') &&
			!(page.route.id ?? '').startsWith('/settings/account/flights')
	);

	/**
	 * A flight fills a narrow screen: the bar steps aside, as the iPhone's tab
	 * bar does, and the screen's own back button leads out.
	 */
	const covered = $derived(
		page.route.id === '/settings/account/flights/flight' || page.route.id === '/flights/[id]'
	);
</script>

<nav class:covered aria-label="Sections">
	<a class="brand" href={homeHref()}>
		<!-- By the theme, the system's or the one chosen in Settings: both are
		     drawn and the style sheet shows one, before any script runs. -->
		<img class="on-light" src={logoOnLight} alt="Venti" width="97" height="28" />
		<img class="on-dark" src={logoOnDark} alt="Venti" width="97" height="28" />
	</a>
	<ul>
		{#each sections as section (section.name)}
			<li class={section.on}>
				<a
					href={section.href()}
					aria-label={section.breaks
						? [section.name, section.root === '/settings' ? unread?.label : undefined]
								.filter(Boolean)
								.join(' ')
						: undefined}
					aria-current={current === section.name ? 'page' : undefined}
				>
					<span class="icon">
						<Icon name={section.icon} />
						{#if section.root === '/settings' && unread?.badge}
							<span class="bubble numeric" aria-hidden="true">{unread.badge}</span>
						{/if}
					</span>
					<span class="name">{section.breaks ?? section.name}</span>
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
			{#if account}
				<span class="pictured">
					<PilotFace
						picture={account.picturePath}
						name={account.displayName || account.handle}
						size="xs"
					/>
					{#if unread?.badge}
						<span class="bubble numeric" aria-hidden="true">{unread.badge}</span>
					{/if}
				</span>
				<span class="handle">{account.handle}</span>
				{#if unread?.label}
					<span class="visually-hidden">{unread.label}</span>
				{/if}
			{:else}
				<span class="icon"><Icon name="person" /></span>
				<span>Sign in</span>
			{/if}
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

	/* What is new: a small bubble at the top right of the picture or the
	   icon, ringed in the bar's colour so that it stands off what it sits on. */
	.icon,
	.pictured {
		position: relative;
		display: inline-flex;
	}

	.bubble {
		position: absolute;
		top: calc(-1 * var(--space-1));
		right: calc(-1 * var(--space-2));
		min-width: var(--space-4);
		height: var(--space-4);
		padding: 0 var(--space-1);
		border-radius: var(--radius-pill);
		box-shadow: 0 0 0 2px var(--surface);
		background: var(--accent);
		color: var(--on-accent);
		font-size: var(--caption);
		font-weight: 600;
		line-height: var(--space-4);
		text-align: center;
		white-space: nowrap;
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

	/* No name is cut short. The caption is the pack's smallest size, so a
	   name wider than its sixth of the bar (at 360 px with text at 130 %)
	   takes a second line under the icon, broken where its soft hyphen is,
	   and the bar grows with it. */
	.name {
		max-width: 100%;
		line-height: calc(16 / 12);
		text-align: center;
		overflow-wrap: anywhere;
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

		.brand img.on-dark,
		:global(:root[data-theme='dark']) .brand img.on-light {
			display: none;
		}

		:global(:root[data-theme='dark']) .brand img.on-dark {
			display: block;
		}

		@media (prefers-color-scheme: dark) {
			:global(:root:not([data-theme])) .brand img.on-light {
				display: none;
			}

			:global(:root:not([data-theme])) .brand img.on-dark {
				display: block;
			}
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

		/* The pilot's face in place of the icon, nearer the round end. */
		.account:has(:global(.face)) {
			padding-left: var(--space-1);
		}

		.name {
			white-space: nowrap;
		}

		/* A long handle is cut short on the wide bar, not wrapped. */
		.end a span:last-child,
		.end a .handle {
			max-width: 100%;
			overflow: hidden;
			text-overflow: ellipsis;
			white-space: nowrap;
		}
	}
</style>
