<!--
	One news post: its picture, its title and day, and its text. A post the
	owner has not finished says "Draft".
-->
<script lang="ts">
	import { newsHref } from '#lib/app/routes.ts';
	import { format } from '#lib/design/format.ts';
	import DraftMark from '#lib/news/DraftMark.svelte';
	import NewsPicture from '#lib/news/NewsPicture.svelte';
	import PostBody from '#lib/news/PostBody.svelte';
	import { postMoment } from '#lib/news/posts.ts';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	const post = $derived(data.post);
	const text = format();
</script>

<svelte:head>
	<title>{post.title} · Venti</title>
</svelte:head>

<div class="desk news-post">
	<article>
		<a class="back" href={newsHref()}>News</a>
		<header>
			<p class="date">
				{text.day(postMoment(post.date))}
				{#if post.draft}<DraftMark />{/if}
			</p>
			<h1>{post.title}</h1>
		</header>
		{#if post.image}
			<NewsPicture svg={post.image} alt={post.imageAlt} />
		{/if}
		<PostBody blocks={post.blocks} />
	</article>
</div>

<style>
	article {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		max-width: 48rem;
		margin: 0 auto;
	}

	.back {
		align-self: flex-start;
		color: var(--link);
		font-weight: 500;
	}

	header {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}

	.date {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		color: var(--text-secondary);
		font-size: var(--subheadline);
	}

	h1 {
		font-size: var(--hero);
		font-style: italic;
		font-weight: 900;
		line-height: 1.05;
	}
</style>
