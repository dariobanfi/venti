<!--
	Open-source licences: the notices that travel with the code of others
	the site carries. MapLibre GL JS with its licence open, the Rust core's
	crates and the site's npm packages each folded. Read at build time
	(#lib/licences/notices.ts); reached from Data and licences.
-->
<script lang="ts">
	import FoldRow from '#lib/design/FoldRow.svelte';
	import { format } from '#lib/design/format.ts';
	import {
		coreNotices,
		packageGroups,
		packageNotice,
		type PackageGroup
	} from '#lib/licences/notices.ts';

	const text = format();
	const maplibre = packageNotice('maplibre-gl');
	const groups = packageGroups(['maplibre-gl']);

	/** A group's row: its packages with their versions, and the licence. */
	function titleOf(group: PackageGroup): string {
		const names = group.packages.map((each) => `${each.name} ${each.version}`).join(', ');
		const licences = [...new Set(group.packages.map((each) => each.licence))].join(', ');
		return `${names} · ${licences}`;
	}
</script>

<svelte:head>
	<title>Open-source licences · Developers · Venti</title>
</svelte:head>

<h1>Open-source licences</h1>
<p class="lead">Venti is built with the work of others. Their licences travel with it.</p>

{#if maplibre}
	<section aria-labelledby="oss-maplibre" data-notice="maplibre-gl">
		<h2 id="oss-maplibre">MapLibre GL JS</h2>
		<p class="quiet">{maplibre.version} · {maplibre.licence}</p>
		<!-- Focusable, so that a long text scrolls by the keyboard too. -->
		<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
		<pre tabindex="0">{maplibre.text}</pre>
	</section>
{/if}

<section aria-labelledby="oss-core" data-notice="core">
	<h2 id="oss-core">Venti’s core</h2>
	<p class="quiet">
		{coreNotices.crates > 0
			? `Rust, compiled to WebAssembly · ${text.count(coreNotices.crates)} crates`
			: 'Rust, compiled to WebAssembly'}
	</p>
	<FoldRow title="The notices">
		<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
		<pre tabindex="0">{coreNotices.text}</pre>
	</FoldRow>
</section>

<section aria-labelledby="oss-packages" data-notice="packages">
	<h2 id="oss-packages">The site’s packages</h2>
	<ul class="groups">
		{#each groups as group (group.packages[0].name)}
			<li>
				<FoldRow title={titleOf(group)}>
					<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
					<pre tabindex="0">{group.text}</pre>
				</FoldRow>
			</li>
		{/each}
	</ul>
</section>

<style>
	section {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		margin-bottom: var(--space-6);
		overflow-wrap: anywhere;
	}

	h2 {
		font-size: var(--title);
	}

	.quiet {
		color: var(--text-secondary);
	}

	pre {
		max-height: 24rem;
		overflow-y: auto;
		padding: var(--space-3);
		border-radius: var(--radius-card);
		background: var(--fill);
		font-size: var(--subheadline);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}

	.groups {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(min(100%, 22rem), 1fr));
		align-items: start;
		gap: var(--space-2);
	}
</style>
