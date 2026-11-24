<!--
	What a JSON Schema of the API's description, or of a tool's arguments,
	says: a named schema as a link to it, a list of what, one of which, an
	object's fields with their kinds and whether they must be there, the
	values a field has today, and every description as the description
	words it.
-->
<script lang="ts">
	import { schemaHref } from '#lib/app/routes.ts';
	import InlineText from '#lib/news/InlineText.svelte';
	import { inline } from '#lib/news/markdown.ts';
	import { boundsOf, kindOf, refName, type Schema } from './reference.ts';
	import SchemaView from './SchemaView.svelte';

	interface Props {
		schema: Schema;
		/** Whether the schema's own description is shown under it. */
		described?: boolean;
		/** Whether a named schema is a link to the page of the schemas; not for a tool's arguments. */
		linked?: boolean;
	}

	let { schema, described = true, linked = true }: Props = $props();

	const alternatives = $derived(schema.anyOf ?? schema.oneOf ?? schema.allOf);
	const values = $derived(
		schema.enum?.map((value) => JSON.stringify(value)) ?? schema['x-venti-values'] ?? []
	);
</script>

{#if schema.$ref}
	{#if linked}
		<a class="ref" href={schemaHref(refName(schema.$ref))}><code>{refName(schema.$ref)}</code></a>
	{:else}
		<code>{refName(schema.$ref)}</code>
	{/if}
{:else if schema.items}
	<span class="kind">list of</span>
	<SchemaView schema={schema.items} described={false} {linked} />
	{#if boundsOf(schema)}<span class="kind">{boundsOf(schema)}</span>{/if}
{:else if alternatives}
	<span class="kind">{schema.allOf ? 'all of' : 'one of'}</span>
	<ul class="nested">
		{#each alternatives as alternative, index (index)}
			<li><SchemaView schema={alternative} {linked} /></li>
		{/each}
	</ul>
{:else if schema.properties}
	<span class="kind">object</span>
	<ul class="nested fields">
		{#each Object.entries(schema.properties) as [name, field] (name)}
			<li>
				<code class="name">{name}</code>
				{#if schema.required?.includes(name)}<span class="required">required</span>{/if}
				<SchemaView schema={field} {linked} />
			</li>
		{/each}
	</ul>
{:else}
	<span class="kind">{kindOf(schema)}</span>
	{#if boundsOf(schema)}<span class="kind">{boundsOf(schema)}</span>{/if}
{/if}
{#if values.length > 0}
	<p class="values">
		{schema.enum ? 'One of' : 'Today'}:
		{#each values as value (value)}<code>{value}</code>{/each}
	</p>
{/if}
{#if described && schema.description}
	<p class="about"><InlineText pieces={inline(schema.description)} /></p>
{/if}

<style>
	.kind {
		color: var(--text-secondary);
	}

	.ref {
		color: var(--link);
		text-decoration: underline;
	}

	code {
		padding: 0 var(--space-1);
		border-radius: var(--radius-s);
		background: var(--fill);
		font-size: var(--subheadline);
		overflow-wrap: anywhere;
	}

	.name {
		font-weight: 600;
	}

	.required {
		color: var(--text-secondary);
		font-size: var(--caption);
	}

	.nested {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		margin-top: var(--space-2);
		padding-left: var(--space-3);
		border-left: 2px solid var(--separator);
	}

	.about,
	.values {
		overflow-wrap: anywhere;
		margin-top: var(--space-1);
		color: var(--text-secondary);
		line-height: 1.5;
	}

	.values {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-1);
	}
</style>
