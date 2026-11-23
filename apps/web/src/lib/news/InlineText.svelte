<!--
	A line's pieces as `markdown.ts` read them, set as text: its links, code,
	strong and emphasised words. A link to another site, or to the server's
	API, leaves the app.
-->
<script lang="ts">
	import type { Inline } from './markdown.ts';

	let { pieces }: { pieces: Inline[] } = $props();

	/** Whether a link leaves the app: another site, or the server's own files. */
	function leaves(href: string): boolean {
		return /^[a-z]+:/i.test(href) || href.startsWith('/v1/');
	}
</script>

{#each pieces as piece, index (index)}
	{#if piece.kind === 'link'}
		<a href={piece.href} rel={leaves(piece.href) ? 'external noopener' : undefined}
			>{#if piece.code}<code>{piece.text}</code>{:else}{piece.text}{/if}</a
		>
	{:else if piece.kind === 'strong'}
		<strong>{piece.text}</strong>
	{:else if piece.kind === 'em'}
		<em>{piece.text}</em>
	{:else if piece.kind === 'code'}
		<code>{piece.text}</code>
	{:else}
		{piece.text}
	{/if}
{/each}

<style>
	a {
		color: var(--link);
		text-decoration: underline;
	}

	strong {
		font-weight: 600;
	}

	code {
		padding: 0 var(--space-1);
		border-radius: var(--radius-s);
		background: var(--fill);
		font-size: var(--subheadline);
		overflow-wrap: anywhere;
	}
</style>
