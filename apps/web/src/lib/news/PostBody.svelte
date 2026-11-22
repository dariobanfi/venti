<!--
	A news post's text, block by block, as `markdown.ts` read it. Every word
	is set as text; a link to another site, or to the server's API, leaves
	the app.
-->
<script lang="ts">
	import type { Block, Inline } from './markdown.ts';

	interface Props {
		blocks: Block[];
	}

	let { blocks }: Props = $props();

	/** Whether a link leaves the app: another site, or the server's own files. */
	function leaves(href: string): boolean {
		return /^[a-z]+:/i.test(href) || href.startsWith('/v1/');
	}
</script>

{#snippet line(pieces: Inline[])}
	{#each pieces as piece, index (index)}
		{#if piece.kind === 'link'}
			{#if leaves(piece.href)}
				<a href={piece.href} rel="external noopener">{piece.text}</a>
			{:else}
				<a href={piece.href}>{piece.text}</a>
			{/if}
		{:else if piece.kind === 'strong'}
			<strong>{piece.text}</strong>
		{:else if piece.kind === 'em'}
			<em>{piece.text}</em>
		{:else}
			{piece.text}
		{/if}
	{/each}
{/snippet}

<div class="post-body">
	{#each blocks as block, index (index)}
		{#if block.kind === 'heading'}
			<h2>{@render line(block.line)}</h2>
		{:else if block.kind === 'list'}
			<ul>
				{#each block.items as item, at (at)}
					<li>{@render line(item)}</li>
				{/each}
			</ul>
		{:else}
			<p>{@render line(block.line)}</p>
		{/if}
	{/each}
</div>

<style>
	.post-body {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		max-width: 40rem;
		font-size: var(--body);
		line-height: 1.6;
	}

	h2 {
		margin-top: var(--space-2);
		font-size: var(--title);
		line-height: 1.3;
	}

	ul {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding-left: var(--space-5);
		list-style: disc;
	}

	strong {
		font-weight: 600;
	}

	a {
		color: var(--link);
		text-decoration: underline;
	}
</style>
