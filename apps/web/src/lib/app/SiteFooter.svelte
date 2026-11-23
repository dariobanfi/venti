<!--
	The foot of the site's pages that are read as pages (home, about, news,
	the developers' pages): what Venti is beside the app's sections, and the
	code. The screens of the app (the map, Today, a flight) have none.
-->
<script lang="ts">
	import { aboutHref, developersHref, newsHref, SOURCE_CODE } from './routes.ts';
</script>

<footer class="site-footer">
	<nav aria-label="About Venti">
		<ul>
			<li><a href={aboutHref()}>About</a></li>
			<li><a href={newsHref()}>News</a></li>
			<li><a href={developersHref()}>Developers</a></li>
			<li><a href={SOURCE_CODE} rel="external noopener">Source code</a></li>
		</ul>
	</nav>
</footer>

<style>
	.site-footer {
		margin-top: var(--space-7);
		padding-top: var(--space-4);
		border-top: 1px solid var(--separator);
	}

	ul {
		display: flex;
		flex-wrap: wrap;
		gap: 0 var(--space-5);
	}

	a {
		display: inline-flex;
		align-items: center;
		min-height: var(--target);
		color: var(--text-secondary);
		font-size: var(--subheadline);
	}

	a:hover {
		color: var(--link);
		text-decoration: underline;
	}
</style>
