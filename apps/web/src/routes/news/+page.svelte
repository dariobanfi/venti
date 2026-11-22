<!--
	Venti's news, the newest first: each post's picture, day, title and the
	lines that say what it is about.
-->
<script lang="ts">
	import { newsPostHref } from '#lib/app/routes.ts';
	import { format } from '#lib/design/format.ts';
	import DraftMark from '#lib/news/DraftMark.svelte';
	import NewsPicture from '#lib/news/NewsPicture.svelte';
	import { postMoment, posts } from '#lib/news/posts.ts';

	const text = format();
</script>

<svelte:head>
	<title>News · Venti</title>
</svelte:head>

<div class="desk">
	<header class="page-head">
		<h1>News</h1>
	</header>
	{#if posts.length === 0}
		<p class="quiet">No news yet.</p>
	{:else}
		<ul class="posts">
			{#each posts as post (post.slug)}
				<li>
					{#if post.image}
						<a class="picture" href={newsPostHref(post.slug)} tabindex="-1" aria-hidden="true">
							<NewsPicture svg={post.image} alt={post.imageAlt} />
						</a>
					{/if}
					<div class="words">
						<p class="date">
							{text.day(postMoment(post.date))}
							{#if post.draft}<DraftMark />{/if}
						</p>
						<h2><a href={newsPostHref(post.slug)}>{post.title}</a></h2>
						{#if post.summary}<p class="quiet">{post.summary}</p>{/if}
					</div>
				</li>
			{/each}
		</ul>
	{/if}
</div>

<style>
	.posts {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(min(100%, 22rem), 1fr));
		gap: var(--space-5);
	}

	li {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}

	.picture {
		display: block;
	}

	.words {
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

	h2 {
		font-size: var(--title);
		line-height: 1.3;
	}

	h2 a:hover {
		text-decoration: underline;
	}

	.quiet {
		color: var(--text-secondary);
	}
</style>
