<!--
	The connector (MCP): its address, how an assistant signs in to read a
	pilot's own logbook, and its tools as the server the site is served by
	lists them (`docs/mcp-tools.json`, read at build time), each with what
	it is for and its arguments.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { CONNECTOR_PATH, tools } from '#lib/developers/connector.ts';
	import { apiDocsHref } from '#lib/developers/reference.ts';
	import SchemaView from '#lib/developers/SchemaView.svelte';
	import { SOURCE_CODE } from '#lib/app/routes.ts';

	let origin = $state('');

	onMount(() => {
		origin = location.origin;
	});

	const MCP_DOCS = `${SOURCE_CODE}/blob/main/docs/mcp.md`;
</script>

<svelte:head>
	<title>Connector · Developers · Venti</title>
</svelte:head>

<h1>Connector (MCP)</h1>
<p class="lead">
	Add Venti to Claude, ChatGPT or any program that speaks the Model Context Protocol. The public
	tools need no sign-in.
</p>

<div class="columns">
	<section class="card span-6" aria-labelledby="mcp-address">
		<h2 id="mcp-address">Address</h2>
		<p><code data-testid="connector-address">{origin}{CONNECTOR_PATH}</code></p>
		<p class="quiet">Streamable HTTP: one JSON-RPC message per <code>POST</code>.</p>
		<a class="text-link" href={MCP_DOCS} rel="external noopener">The connector in full</a>
	</section>

	<section class="card span-6" aria-labelledby="mcp-sign-in">
		<h2 id="mcp-sign-in">Signing in</h2>
		<p>
			A pilot’s own logbook needs the pilot’s yes: OAuth 2.1 with PKCE. The assistant sends the
			pilot to Venti, who agrees on the page <code>/connect/</code>, signed in as they are. The
			token has one scope, <code>flights:read</code>, and the pilot withdraws it in their account.
		</p>
		<a
			class="text-link"
			href={apiDocsHref('How an assistant connects, step by step')}
			rel="external noopener">Step by step</a
		>
	</section>
</div>

<section class="tools" aria-labelledby="mcp-tools">
	<h2 id="mcp-tools">Tools</h2>
	{#if tools.length === 0}
		<p class="quiet">This server offers no tools.</p>
	{:else}
		<ul>
			{#each tools as tool (tool.name)}
				<li class="tool" data-tool={tool.name}>
					<h3><code>{tool.name}</code></h3>
					{#if tool.annotations?.readOnlyHint}<p class="quiet">Only reads</p>{/if}
					<p>{tool.description}</p>
					<div class="arguments">
						<h4>Arguments</h4>
						{#if Object.keys(tool.inputSchema.properties ?? {}).length === 0}
							<p class="quiet">None</p>
						{:else}
							<SchemaView schema={tool.inputSchema} linked={false} />
						{/if}
					</div>
				</li>
			{/each}
		</ul>
	{/if}
</section>

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

	code {
		padding: 0 var(--space-1);
		border-radius: var(--radius-s);
		background: var(--fill);
		font-size: var(--subheadline);
		overflow-wrap: anywhere;
	}

	.quiet {
		color: var(--text-secondary);
	}

	.tools {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		margin-top: var(--space-6);
	}

	.tool {
		overflow-wrap: anywhere;
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-4) 0;
		border-top: 1px solid var(--separator);
		line-height: 1.5;
	}

	h3 {
		font-size: var(--body);
	}

	h3 code {
		font-weight: 600;
	}

	h4 {
		font-size: var(--subheadline);
		font-weight: 600;
	}

	.arguments {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}
</style>
