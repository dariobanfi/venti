<!--
	Start: what Venti's data is and who may use it (owner decision 16), where
	the API is, its bounds and what is promised, in a few lines, and the way
	to the other pages.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { API_DESCRIPTION, developersHref } from '#lib/app/routes.ts';
	import { apiDocsHref } from '#lib/developers/reference.ts';
	import { ADDRESS_BUDGET, TOKEN_BUDGET } from '#lib/developers/words.ts';

	// The API is at the root of the origin the site is served from, which
	// the page knows only once it runs in a browser.
	let origin = $state('');
	onMount(() => (origin = location.origin));
</script>

<svelte:head>
	<title>Developers · Venti</title>
</svelte:head>

<h1>Venti for developers</h1>
<p class="lead">
	Everything Venti holds is open: wind stations, airspace, flying sites, forecasts, public flights
	and pilots’ pages. Read it with no account and no key. Only flights a pilot keeps private stay
	private.
</p>

<div class="columns">
	<section class="card span-6" aria-labelledby="dev-address">
		<h2 id="dev-address">The API</h2>
		<dl>
			<dt>Base address</dt>
			<dd><code data-testid="base-address">{origin}/v1</code></dd>
			<dt>Description</dt>
			<dd>
				<a class="text-link" href={API_DESCRIPTION} rel="external">OpenAPI 3.1</a>
			</dd>
			<dt>Try it</dt>
			<dd><code>curl {origin}/v1/health</code></dd>
		</dl>
	</section>

	<section class="card span-6" aria-labelledby="dev-bounds">
		<h2 id="dev-bounds">Bounds</h2>
		<ul>
			<li>A token for a program: {TOKEN_BUDGET}.</li>
			<li>Public flights without a token: {ADDRESS_BUDGET}.</li>
			<li>On <code>429</code>, wait as long as <code>Retry-After</code> says.</li>
			<li>Send <code>If-None-Match</code>: an unchanged answer costs a <code>304</code>.</li>
		</ul>
		<a class="text-link" href={apiDocsHref('Limits')} rel="external noopener">All the bounds</a>
	</section>

	<section class="card span-12" aria-labelledby="dev-promise">
		<h2 id="dev-promise">What is promised</h2>
		<p>
			Within <code>/v1</code> a path stays, a field keeps its name, type and meaning, and an error
			keeps its <code>code</code>. New fields and new values may come: ignore what you do not know.
			Every answer names its sources in <code>attributions</code>: show the credit beside the data.
		</p>
	</section>
</div>

<ul class="pages">
	<li>
		<a href={developersHref('api')}
			><strong>REST API</strong><span>Every operation and schema</span></a
		>
	</li>
	<li>
		<a href={developersHref('connector')}
			><strong>Connector (MCP)</strong><span>For an assistant: its tools and sign-in</span></a
		>
	</li>
	<li>
		<a href={developersHref('data')}
			><strong>Data and licences</strong><span>Each source’s credit, and the downloads</span></a
		>
	</li>
	<li>
		<a href={developersHref('add-a-source')}
			><strong>Add a data source</strong><span>Bring your country’s stations</span></a
		>
	</li>
</ul>

<style>
	.card {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		padding: var(--space-4);
		border: 1px solid var(--separator);
		border-radius: var(--radius-card);
		background: var(--surface);
		line-height: 1.5;
	}

	h2 {
		font-size: var(--title);
	}

	dl {
		display: grid;
		grid-template-columns: auto minmax(0, 1fr);
		gap: var(--space-2) var(--space-4);
	}

	dt {
		color: var(--text-secondary);
	}

	dd {
		min-width: 0;
		overflow-wrap: anywhere;
	}

	ul:not(.pages) {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		padding-left: var(--space-5);
		list-style: disc;
	}

	code {
		padding: 0 var(--space-1);
		border-radius: var(--radius-s);
		background: var(--fill);
		font-size: var(--subheadline);
	}

	.pages {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(min(100%, 16rem), 1fr));
		gap: var(--space-3);
		margin-top: var(--space-5);
	}

	.pages a {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		height: 100%;
		padding: var(--space-4);
		border: 1px solid var(--separator);
		border-radius: var(--radius-card);
		color: var(--text);
	}

	.pages a:hover {
		border-color: var(--color-border-strong);
	}

	.pages strong {
		color: var(--link);
		font-weight: 600;
	}

	.pages span {
		color: var(--text-secondary);
	}
</style>
