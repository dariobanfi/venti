<!--
	A news post's picture: the owner's SVG, set into the page as it is
	written, so that its classes take the design pack's brand colours. The
	file writes no colour of its own and no style attribute (the content
	security policy allows none): a shape takes a colour by its class,
	`f-<colour>` for its fill, `s-<colour>` for its stroke and `k-<colour>`
	for a gradient's stop. A colour not listed below needs its line here.

	The picture is the repository's own file, never anything a pilot wrote.
-->
<script lang="ts">
	interface Props {
		svg: string;
		/** What the picture shows, for whoever cannot see it. */
		alt: string;
	}

	let { svg, alt }: Props = $props();
</script>

<div class="news-picture" role="img" aria-label={alt}>
	{@html svg}
</div>

<style>
	.news-picture {
		overflow: hidden;
		aspect-ratio: 2 / 1;
		border-radius: var(--radius-card);
		background: var(--brand-cream);
	}

	.news-picture :global(svg) {
		display: block;
		width: 100%;
		height: 100%;
	}

	.news-picture :global(.f-ink) {
		fill: var(--brand-ink);
	}
	.news-picture :global(.f-indigo) {
		fill: var(--brand-indigo);
	}
	.news-picture :global(.f-purple) {
		fill: var(--brand-purple);
	}
	.news-picture :global(.f-violet) {
		fill: var(--brand-violet);
	}
	.news-picture :global(.f-lilac) {
		fill: var(--brand-lilac);
	}
	.news-picture :global(.f-mint) {
		fill: var(--brand-mint);
	}
	.news-picture :global(.f-sage) {
		fill: var(--brand-sage);
	}
	.news-picture :global(.f-cream) {
		fill: var(--brand-cream);
	}
	.news-picture :global(.f-sand) {
		fill: var(--brand-sand);
	}
	.news-picture :global(.f-peach) {
		fill: var(--brand-peach);
	}
	.news-picture :global(.f-gold) {
		fill: var(--brand-gold);
	}
	.news-picture :global(.f-coral) {
		fill: var(--brand-coral);
	}
	.news-picture :global(.s-ink) {
		stroke: var(--brand-ink);
	}
	.news-picture :global(.s-purple) {
		stroke: var(--brand-purple);
	}
	.news-picture :global(.s-brick) {
		stroke: var(--brand-brick);
	}
	.news-picture :global(.k-sky) {
		stop-color: var(--brand-sky);
	}
	.news-picture :global(.k-mint) {
		stop-color: var(--brand-mint);
	}
	.news-picture :global(.k-sage) {
		stop-color: var(--brand-sage);
	}
	.news-picture :global(.k-cream) {
		stop-color: var(--brand-cream);
	}
</style>
