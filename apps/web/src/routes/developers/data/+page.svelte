<!--
	Data and licences: every source the server is built with, its licence
	and the credit its provider asks for, read at build time from the
	sources' manifests; and Venti's own sites whole, as the server lists
	its dumps when the page opens. A link to the open-source licences of
	the code the site carries.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { format } from '#lib/design/format.ts';
	import { listDumps, type DumpLayer } from '#lib/developers/dumps.ts';
	import { sources } from '#lib/developers/manifests.ts';
	import { developersHref } from '#lib/app/routes.ts';
	import { apiDocsHref } from '#lib/developers/reference.ts';

	const text = format();

	let dumps = $state<DumpLayer[] | undefined>();
	let failed = $state(false);

	async function load(): Promise<void> {
		failed = false;
		dumps = undefined;
		try {
			dumps = await listDumps();
		} catch {
			failed = true;
		}
	}

	onMount(() => void load());

	/** A file's size, as "1.7 kB". */
	function size(bytes: number): string {
		if (bytes < 1000) return `${bytes} B`;
		if (bytes < 1_000_000) return `${text.number(Math.round(bytes / 100) / 10)} kB`;
		return `${text.number(Math.round(bytes / 100_000) / 10)} MB`;
	}
</script>

<svelte:head>
	<title>Data and licences · Developers · Venti</title>
</svelte:head>

<h1>Data and licences</h1>
<p class="lead">
	Each source keeps its own licence, and its records are never mixed with another’s. Show the credit
	beside the data.
</p>
<p class="lead">
	The code the site is built with:
	<a class="text-link" href={developersHref('data/open-source')}>Open-source licences</a>
</p>

<section aria-labelledby="data-downloads">
	<h2 id="data-downloads">Downloads</h2>
	<p class="quiet">Venti’s own sites, whole. Then follow the changes from the file’s cursor.</p>
	{#if failed}
		<div class="problem" role="alert">
			<p>The server could not be asked for its downloads.</p>
			<button class="button-plain" type="button" onclick={load}>Try again</button>
		</div>
	{:else if !dumps}
		<p class="quiet" role="status">Loading…</p>
	{:else if dumps.length === 0}
		<p class="quiet">This server has no downloads.</p>
	{:else}
		<ul class="dumps">
			{#each dumps as layer (layer.layer)}
				<li class="card" data-layer={layer.layer}>
					<h3>{layer.provider} <code>{layer.layer}</code></h3>
					<p>
						<a class="text-link" href={layer.licence_url} rel="external noopener">{layer.licence}</a
						>
						· {text.count(layer.sites)}
						{layer.sites === 1 ? 'site' : 'sites'}
					</p>
					{#if layer.notice}<p class="quiet">{layer.notice}</p>{/if}
					<ul class="files">
						{#each layer.files as file (file.path)}
							<li>
								<a class="text-link" href={file.path} rel="external" download>{file.format}</a>
								<span class="quiet">{file.content}, {size(file.bytes)}</span>
							</li>
						{/each}
					</ul>
				</li>
			{/each}
		</ul>
	{/if}
	<a class="text-link" href={apiDocsHref("Dumps of Venti's own sites")} rel="external noopener"
		>About the files</a
	>
</section>

<section aria-labelledby="data-sources">
	<h2 id="data-sources">Sources</h2>
	<ul class="sources">
		{#each sources as source (source.id)}
			<li class="card" data-source={source.id}>
				<h3>{source.provider} <code>{source.id}</code></h3>
				<p>
					<a class="text-link" href={source.licence.link} rel="external noopener"
						>{source.licence.name}</a
					>{#if !source.licence.commercial}
						· Non-commercial{/if}{#if source.licence.shareAlike}
						· Share-alike{/if}
				</p>
				<p class="credit">{source.licence.credit}</p>
				{#each source.assets as asset (asset.what)}
					<p class="quiet">
						Built with {asset.what}: {asset.licence.name}, {asset.licence.credit}
					</p>
				{/each}
				<p class="quiet">{source.kinds.join(', ')} · read {source.licence.verifiedOn}</p>
			</li>
		{/each}
	</ul>
</section>

<style>
	section {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: var(--space-3);
		margin-bottom: var(--space-6);
	}

	h2 {
		font-size: var(--title);
	}

	h3 {
		font-size: var(--body);
		font-weight: 600;
	}

	code {
		padding: 0 var(--space-1);
		border-radius: var(--radius-s);
		background: var(--fill);
		font-size: var(--subheadline);
		font-weight: 400;
	}

	.quiet {
		color: var(--text-secondary);
	}

	.dumps,
	.sources {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(min(100%, 22rem), 1fr));
		gap: var(--space-3);
		width: 100%;
	}

	.card {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		min-width: 0;
		padding: var(--space-4);
		border: 1px solid var(--separator);
		border-radius: var(--radius-card);
		background: var(--surface);
		line-height: 1.5;
		overflow-wrap: anywhere;
	}

	.files {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}

	.files a {
		font-weight: 600;
	}

	.problem {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: var(--space-2);
	}
</style>
