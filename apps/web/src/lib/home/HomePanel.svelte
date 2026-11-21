<!--
	One panel of the home page: its title, the way to the screen it is a
	glance of, and what it holds. While its answer is on the way it says
	so; when it cannot be had it says why, with "Try again"; with no Venti
	behind the app it says that in one line.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		id: string;
		title: string;
		/** The way on: where, and in what words. */
		more?: { href: string; words: string };
		/** Whether the answer is on its way. */
		loading?: boolean;
		/** Why it could not be had. */
		problem?: string;
		/** Whether the app has no Venti behind it. */
		absent?: boolean;
		again?: () => void;
		class?: string;
		children?: Snippet;
	}

	let {
		id,
		title,
		more,
		loading = false,
		problem,
		absent = false,
		again,
		class: className = '',
		children
	}: Props = $props();
</script>

<section class="panel {className}" aria-labelledby={id} aria-busy={loading}>
	<header>
		<h2 {id}>{title}</h2>
		{#if more}
			<a class="more" href={more.href}>{more.words}</a>
		{/if}
	</header>
	{#if loading}
		<p class="quiet">Loading…</p>
	{:else if problem}
		<p class="quiet">{problem}</p>
		{#if again}
			<button class="button-plain" type="button" onclick={again}>Try again</button>
		{/if}
	{:else if absent}
		<p class="quiet">Not connected to Venti.</p>
	{:else}
		{@render children?.()}
	{/if}
</section>

<style>
	.panel {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		min-width: 0;
		padding: var(--space-3) var(--space-4) var(--space-4);
		background: var(--surface);
		border: 1px solid var(--separator);
		border-radius: var(--radius-card);
	}

	header {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: var(--space-3);
	}

	h2 {
		font-size: var(--title);
		line-height: 1.3;
	}

	.more {
		flex: none;
		color: var(--link);
		font-size: var(--subheadline);
		font-weight: 500;
	}

	@media (hover: hover) {
		.more:hover {
			text-decoration: underline;
		}
	}

	.quiet {
		color: var(--text-secondary);
	}

	.button-plain {
		align-self: flex-start;
		margin-left: calc(-1 * var(--space-3));
	}
</style>
