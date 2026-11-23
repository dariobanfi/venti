<!--
	One operation of the REST API as its description has it: the method and
	the path with what it does, folded; unfolded, who may ask, the
	parameters, the request's body, each answer with its schema, the
	examples the description carries, and the heading of docs/api.md that
	says it in words.
-->
<script lang="ts">
	import InlineText from '#lib/news/InlineText.svelte';
	import { inline } from '#lib/news/markdown.ts';
	import type { Body, Operation } from './reference.ts';
	import SchemaView from './SchemaView.svelte';

	let { operation }: { operation: Operation } = $props();
</script>

{#snippet bodies(list: Body[])}
	{#each list as body (body.mediaType)}
		<div class="body">
			<p class="media"><code>{body.mediaType}</code></p>
			{#if body.schema}<div><SchemaView schema={body.schema} /></div>{/if}
			{#each body.examples as example (example.name)}
				<p class="example-name">{example.name}</p>
				<!-- Focusable, so that an example wider than the column scrolls by the keyboard too. -->
				<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
				<pre tabindex="0"><code>{example.json}</code></pre>
			{/each}
		</div>
	{/each}
{/snippet}

<details class="operation" id={operation.id} data-operation="{operation.method} {operation.path}">
	<summary>
		<span class="method">{operation.method}</span>
		<code class="path">{operation.path}</code>
		<span class="summary"><InlineText pieces={inline(operation.summary)} /></span>
	</summary>
	<div class="detail">
		{#if operation.description}
			<p><InlineText pieces={inline(operation.description)} /></p>
		{/if}

		{#if operation.parameters.length > 0}
			<h4>Parameters</h4>
			<ul class="list">
				{#each operation.parameters as parameter (`${parameter.in} ${parameter.name}`)}
					<li>
						<code class="name">{parameter.name}</code>
						<span class="quiet">in {parameter.in}{parameter.required ? ', required' : ''}</span>
						{#if parameter.schema}<SchemaView schema={parameter.schema} />{/if}
						{#if parameter.description}
							<p class="quiet"><InlineText pieces={inline(parameter.description)} /></p>
						{/if}
					</li>
				{/each}
			</ul>
		{/if}

		{#if operation.request.length > 0}
			<h4>Request body</h4>
			{@render bodies(operation.request)}
		{/if}

		<h4>Answers</h4>
		<ul class="list">
			{#each operation.responses as response (response.status)}
				<li>
					<span class="status">{response.status === 'default' ? 'Error' : response.status}</span>
					<span class="quiet"><InlineText pieces={inline(response.description)} /></span>
					{@render bodies(response.bodies)}
				</li>
			{/each}
		</ul>

		{#if operation.docs}
			<p><a href={operation.docs} rel="external noopener">In words, in api.md</a></p>
		{/if}
	</div>
</details>

<style>
	.operation {
		border-top: 1px solid var(--separator);
	}

	summary {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: var(--space-1) var(--space-2);
		min-height: var(--target);
		padding: var(--space-3) 0;
		cursor: pointer;
	}

	summary:focus-visible {
		outline: 2px solid var(--color-focus);
		outline-offset: 2px;
	}

	.method {
		min-width: 4rem;
		font-size: var(--subheadline);
		font-weight: 600;
	}

	.path {
		font-weight: 600;
		overflow-wrap: anywhere;
	}

	.summary {
		flex-basis: 100%;
		color: var(--text-secondary);
	}

	@media (min-width: 56rem) {
		.summary {
			flex-basis: auto;
		}
	}

	.detail {
		overflow-wrap: anywhere;
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding-bottom: var(--space-4);
		line-height: 1.5;
	}

	h4 {
		margin-top: var(--space-2);
		font-size: var(--subheadline);
		font-weight: 600;
	}

	.list {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}

	.name {
		font-weight: 600;
	}

	.quiet {
		color: var(--text-secondary);
	}

	.status {
		font-weight: 600;
	}

	.body {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		margin-top: var(--space-1);
	}

	code {
		padding: 0 var(--space-1);
		border-radius: var(--radius-s);
		background: var(--fill);
		font-size: var(--subheadline);
	}

	pre {
		overflow-x: auto;
		max-height: 24rem;
		padding: var(--space-3);
		border-radius: var(--radius-card);
		background: var(--fill);
		font-size: var(--subheadline);
	}

	pre code {
		padding: 0;
		white-space: pre;
	}

	.example-name {
		margin-top: var(--space-2);
		font-weight: 600;
	}

	a {
		color: var(--link);
		text-decoration: underline;
	}
</style>
