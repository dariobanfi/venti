<!--
	What Venti stands for, as a band across the home page: four plain lines
	on the pack's indigo with its contour lines, and the way to the about
	page. A band of the site's own, not of the data: the pack's indigo
	section, in its brand colours.
-->
<script lang="ts">
	import { MORE_ABOUT, STANDS_FOR } from '#lib/about/words.ts';
	import { aboutHref } from '#lib/app/routes.ts';

	interface Props {
		class?: string;
	}

	let { class: className = '' }: Props = $props();
</script>

<section class="stands-for {className}" aria-label="What Venti stands for">
	<ul>
		{#each STANDS_FOR as line (line)}
			<li>{line}</li>
		{/each}
	</ul>
	<a href={aboutHref()}>{MORE_ABOUT}</a>
</section>

<style>
	.stands-for {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3) var(--space-5);
		padding: var(--space-4) var(--space-5);
		border-radius: var(--radius-card);
		color: var(--brand-cream);
		background:
			url('../../../../../design/assets/pattern/contours-on-indigo.svg') center / 30rem repeat,
			var(--brand-indigo);
	}

	ul {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-1) var(--space-5);
		font-size: var(--title);
		font-weight: 600;
	}

	a {
		flex: none;
		display: inline-flex;
		align-items: center;
		min-height: var(--target);
		color: var(--brand-cream);
		font-weight: 500;
		text-decoration: underline;
		text-underline-offset: 0.2em;
	}

	a:focus-visible {
		outline: 0.1875rem solid var(--brand-cream);
		outline-offset: 0.125rem;
		border-radius: var(--radius-s);
	}

	/* A phone: the lines one under the other. */
	@media (max-width: 55.99rem) {
		ul {
			flex-direction: column;
		}
	}
</style>
