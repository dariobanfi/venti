<!--
	Every schema of the API's description, written out at build time from
	docs/openapi.json, in the order of their names: what an operation of the
	reference links to.
-->
<script lang="ts">
	import { developersHref } from '#lib/app/routes.ts';
	import { description, schemasOf } from '#lib/developers/reference.ts';
	import SchemaView from '#lib/developers/SchemaView.svelte';
	import InlineText from '#lib/news/InlineText.svelte';
	import { inline } from '#lib/news/markdown.ts';

	const schemas = schemasOf(description);
</script>

<svelte:head>
	<title>Schemas · REST API · Developers · Venti</title>
</svelte:head>

<h1>Schemas</h1>
<p class="lead">
	{schemas.length} records of the <a class="text-link" href={developersHref('api')}>REST API</a>. No
	schema is closed: a record may gain fields.
</p>

<div class="schemas">
	{#each schemas as [name, schema] (name)}
		<section id="schema-{name}" aria-labelledby="schema-{name}-name">
			<h2 id="schema-{name}-name"><code>{name}</code></h2>
			{#if schema.description}
				<p class="about"><InlineText pieces={inline(schema.description)} /></p>
			{/if}
			<div class="shape"><SchemaView {schema} described={false} /></div>
		</section>
	{/each}
</div>

<style>
	.schemas {
		display: flex;
		flex-direction: column;
	}

	section {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-4) 0;
		border-top: 1px solid var(--separator);
		line-height: 1.5;
		scroll-margin-top: calc(var(--nav-top) + var(--space-4));
	}

	section:target {
		background: var(--fill);
	}

	h2 {
		font-size: var(--title);
	}

	h2 code {
		font-weight: 600;
		overflow-wrap: anywhere;
	}

	.about {
		color: var(--text-secondary);
	}
</style>
