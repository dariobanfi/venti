<!--
	Adding a data source: the guide docs/adding-a-source.md as the
	repository has it, read at build time, with the template and the check
	on GitHub over it.
-->
<script lang="ts">
	import { CHECK, guide, TEMPLATE } from '#lib/developers/guide.ts';
	import PostBody from '#lib/news/PostBody.svelte';
</script>

<svelte:head>
	<title>Add a data source · Developers · Venti</title>
</svelte:head>

<h1>Add a data source</h1>
<ul class="start">
	<li><a class="text-link" href={TEMPLATE} rel="external noopener">The template</a></li>
	<li><a class="text-link" href={CHECK} rel="external noopener">The check</a></li>
</ul>

<PostBody blocks={guide} />

<style>
	.start {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-1) var(--space-5);
		margin-bottom: var(--space-5);
	}

	.start a {
		display: inline-flex;
		align-items: center;
		min-height: var(--target);
		font-weight: 600;
	}
</style>
