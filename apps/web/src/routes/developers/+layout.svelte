<!--
	The pages for developers: where Venti's data is and who may use it, the
	REST API, the connector, the sources and their licences, and adding a
	source. Reached from the about page and the site's foot, not from the
	navigation. The pages of this section in a list at the left of a wide
	screen, in a row over the page on a narrow one.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';
	import { page } from '$app/state';
	import { developersHref } from '#lib/app/routes.ts';
	import SiteFooter from '#lib/app/SiteFooter.svelte';

	let { children }: { children: Snippet } = $props();

	const PAGES = [
		{ name: 'Start', href: developersHref() },
		{ name: 'REST API', href: developersHref('api') },
		{ name: 'Connector (MCP)', href: developersHref('connector') },
		{ name: 'Data and licences', href: developersHref('data') },
		{ name: 'Add a data source', href: developersHref('add-a-source') }
	];

	const bare = (path: string) => path.replace(/\/$/u, '');

	function isOpen(href: string): boolean {
		const path = bare(page.url.pathname);
		const start = bare(developersHref());
		return bare(href) === start ? path === start : path.startsWith(bare(href));
	}
</script>

<div class="desk developers">
	<nav aria-label="Developers">
		<ul>
			{#each PAGES as each (each.href)}
				<li>
					<a href={each.href} aria-current={isOpen(each.href) ? 'page' : undefined}>{each.name}</a>
				</li>
			{/each}
		</ul>
	</nav>
	<div class="page">
		{@render children()}
		<SiteFooter />
	</div>
</div>

<style>
	.developers {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}

	ul {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-1) var(--space-2);
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

	.page {
		min-width: 0;
	}

	@media (min-width: 56rem) {
		.developers {
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
			flex-direction: column;
		}
	}

	/* The pages' shared look: a title, short paragraphs, links in the pack's colour. */
	.page :global(h1) {
		margin-bottom: var(--space-4);
		font-size: var(--large-title);
		line-height: 1.25;
	}

	.page :global(.lead) {
		max-width: 40rem;
		margin-bottom: var(--space-5);
		line-height: 1.6;
	}

	.page :global(.text-link) {
		color: var(--link);
		text-decoration: underline;
	}
</style>
