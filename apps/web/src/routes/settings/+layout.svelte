<!--
	The frame of the settings. On a wide screen: the sections in a list at
	the left, the one open at the right, as a website's settings are. On a
	phone the screens stand alone, each with its way back, as on the iPhone.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';
	import { MediaQuery } from 'svelte/reactivity';
	import { page } from '$app/state';
	import { accountHref, airspaceHref, settingsHref } from '#lib/app/routes.ts';

	let { children }: { children: Snippet } = $props();

	const wide = new MediaQuery('min-width: 56rem', false);

	/** The sections, and whether the page open is one of each: the account's own pages are the account's. */
	const SECTIONS = [
		{ name: 'General', href: settingsHref(), exact: true },
		{ name: 'Account', href: accountHref(), exact: false },
		{ name: 'Airspace', href: airspaceHref(), exact: false }
	];

	function isOpen(section: (typeof SECTIONS)[number]): boolean {
		// The address may end in a slash, or not.
		const bare = (path: string) => path.replace(/\/$/u, '');
		const path = bare(page.url.pathname);
		const href = bare(section.href);
		return section.exact ? path === href : path === href || path.startsWith(`${href}/`);
	}
</script>

{#if wide.current}
	<div class="desk frame">
		<nav aria-label="Sections of the settings">
			<ul>
				{#each SECTIONS as section (section.href)}
					<li>
						<a href={section.href} aria-current={isOpen(section) ? 'page' : undefined}>
							{section.name}
						</a>
					</li>
				{/each}
			</ul>
		</nav>
		<div class="section">
			{@render children()}
		</div>
	</div>
{:else}
	{@render children()}
{/if}

<style>
	.frame {
		display: grid;
		grid-template-columns: 14rem minmax(0, 1fr);
		gap: var(--space-6);
		align-items: start;
	}

	nav {
		position: sticky;
		top: calc(var(--nav-top) + var(--space-5));
	}

	ul {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	a {
		display: flex;
		align-items: center;
		min-height: var(--target);
		padding: 0 var(--space-3);
		border-radius: var(--radius-control);
		color: var(--text);
		font-weight: 500;
	}

	a[aria-current='page'] {
		background: var(--fill);
		color: var(--accent);
		font-weight: 600;
	}

	@media (hover: hover) {
		a:hover {
			background: var(--fill);
		}
	}

	/* The section's own page, at the left of the space beside the list and
	   no wider than settings are read. */
	.section :global(.settings-page) {
		margin: 0;
		padding: 0;
	}
</style>
