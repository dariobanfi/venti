<!--
	A news post's text, or a document of the repository's, block by block, as
	`markdown.ts` read it. Every word is set as text; a link to another site,
	or to the server's API, leaves the app. Code and tables that are wider
	than the column scroll within it, never the page.
-->
<script lang="ts">
	import InlineText from './InlineText.svelte';
	import type { Block } from './markdown.ts';

	interface Props {
		blocks: Block[];
	}

	let { blocks }: Props = $props();
</script>

<div class="post-body">
	{#each blocks as block, index (index)}
		{#if block.kind === 'heading'}
			<svelte:element this={`h${Math.max(2, block.level)}`} id={block.id}
				><InlineText pieces={block.line} /></svelte:element
			>
		{:else if block.kind === 'list'}
			<ul>
				{#each block.items as item, at (at)}
					<li><InlineText pieces={item} /></li>
				{/each}
			</ul>
		{:else if block.kind === 'code'}
			<!-- Focusable, so that code wider than the column scrolls by the keyboard too. -->
			<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
			<pre tabindex="0"><code>{block.text}</code></pre>
		{:else if block.kind === 'table'}
			<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
			<div class="table" tabindex="0" role="region" aria-label="Table">
				<table>
					<thead>
						<tr>
							{#each block.head as cell, at (at)}
								<th scope="col"><InlineText pieces={cell} /></th>
							{/each}
						</tr>
					</thead>
					<tbody>
						{#each block.rows as row, at (at)}
							<tr>
								{#each row as cell, column (column)}
									<td><InlineText pieces={cell} /></td>
								{/each}
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		{:else}
			<p><InlineText pieces={block.line} /></p>
		{/if}
	{/each}
</div>

<style>
	.post-body {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		max-width: 40rem;
		min-width: 0;
		font-size: var(--body);
		line-height: 1.6;
		overflow-wrap: anywhere;
	}

	h2 {
		margin-top: var(--space-2);
		font-size: var(--title);
		line-height: 1.3;
	}

	h3,
	h4 {
		margin-top: var(--space-2);
		font-size: var(--body);
		font-weight: 600;
		line-height: 1.4;
	}

	ul {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding-left: var(--space-5);
		list-style: disc;
	}

	pre {
		overflow-x: auto;
		padding: var(--space-3);
		border-radius: var(--radius-card);
		background: var(--fill);
		font-size: var(--subheadline);
		line-height: 1.5;
		overflow-wrap: normal;
	}

	pre code {
		white-space: pre;
	}

	.table {
		overflow-x: auto;
		border: 1px solid var(--separator);
		border-radius: var(--radius-card);
	}

	table {
		min-width: 40rem;
		border-collapse: collapse;
		font-size: var(--subheadline);
		line-height: 1.45;
		overflow-wrap: normal;
	}

	th,
	td {
		padding: var(--space-2) var(--space-3);
		text-align: left;
		vertical-align: top;
	}

	th {
		font-weight: 600;
	}

	tbody tr {
		border-top: 1px solid var(--separator);
	}
</style>
