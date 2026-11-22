<!--
	What Venti stands for, in a few short paragraphs: free and open source,
	no advertising and no tracking, the community's data open to everyone
	through the API, made by pilots. Under the pack's website hero, an
	arrival moment with no data on it. The words are the web's own.
-->
<script lang="ts">
	import { API_DESCRIPTION, newsHref, SOURCE_CODE } from '#lib/app/routes.ts';
	import hero from '../../../../../design/assets/illustration/hero-wide.svg?url';
</script>

<svelte:head>
	<title>About · Venti</title>
</svelte:head>

<div class="desk about">
	<img class="hero" src={hero} alt="" />

	<article>
		<h1>What Venti stands for</h1>
		<p class="lead">
			Venti is one place for paragliding pilots: the forecast for the take-offs, the wind as it is
			measured, a cockpit to fly with, a logbook, and the flights of everyone who shares theirs.
		</p>

		<section aria-labelledby="about-free">
			<h2 id="about-free">Free and open source</h2>
			<p>
				Venti is free. Its code is open: anyone can read it, run it and make it better.
				<a href={SOURCE_CODE} rel="external noopener">The source code</a>
			</p>
		</section>

		<section aria-labelledby="about-no-ads">
			<h2 id="about-no-ads">No advertising, no tracking</h2>
			<p>
				There are no ads here and nothing that follows you around. No one else’s script runs on this
				site, and nothing you do here is sold.
			</p>
		</section>

		<section aria-labelledby="about-data">
			<h2 id="about-data">The data is the community’s</h2>
			<p>
				Take-offs, flights and what pilots know of them belong to everyone who flies. All of it is
				open to anyone through Venti’s API, under each source’s licence. Only the flights you keep
				private stay private.
				<a href={API_DESCRIPTION} rel="external">The API</a>
			</p>
		</section>

		<section aria-labelledby="about-pilots">
			<h2 id="about-pilots">Made by pilots</h2>
			<p>
				Venti is made by pilots, for pilots, and earns nothing. It grows with the people who use it:
				share a flight, add a take-off you know, bring your country’s wind stations.
				<a href={newsHref()}>News</a>
			</p>
		</section>
	</article>
</div>

<style>
	.about {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: var(--space-6);
	}

	.hero {
		display: block;
		width: 100%;
		max-width: 60rem;
		aspect-ratio: 5 / 2;
		object-fit: cover;
		object-position: 60% 30%;
		border-radius: var(--radius-card);
	}

	article {
		display: flex;
		flex-direction: column;
		gap: var(--space-5);
		width: 100%;
		max-width: 40rem;
	}

	h1 {
		font-size: var(--hero);
		font-style: italic;
		font-weight: 900;
		line-height: 1.05;
	}

	.lead {
		font-size: var(--title);
		line-height: 1.45;
	}

	section {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}

	h2 {
		font-size: var(--title);
		line-height: 1.3;
	}

	p {
		line-height: 1.6;
	}

	a {
		color: var(--link);
		font-weight: 500;
		text-decoration: underline;
		text-underline-offset: 0.2em;
	}
</style>
