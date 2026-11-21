<!--
	The key of the map of slopes, in the manner of the forecast map's: the
	five classes with a swatch each in the colour the map draws them in, and
	one line on what the steep classes mean on foot.
-->
<script lang="ts">
	import { SLOPE_KEY } from './grid';
</script>

<section class="slopes-key" aria-labelledby="slopes-key">
	<h3 class="label" id="slopes-key">Slopes</h3>
	<ul class="key">
		{#each SLOPE_KEY as entry (entry.words)}
			<li>
				<span class="swatch" style:background={entry.color} aria-hidden="true"></span>
				{entry.words}
			</li>
		{/each}
	</ul>
	<p class="caption">Steeper than 30° is where snow slides; on foot, take care.</p>
</section>

<style>
	.slopes-key {
		display: grid;
		gap: var(--space-2);
	}

	.label {
		margin: 0;
		color: var(--text-secondary);
		font-size: var(--caption);
		font-weight: 600;
	}

	.key {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(7rem, 1fr));
		gap: var(--space-1) var(--space-3);
		margin: 0;
		padding: 0;
		list-style: none;
		font-size: var(--subheadline);
	}

	.key li {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	/* The cell as the map draws it, with an edge, since the palest steps
	   lie close to the card's surface. */
	.swatch {
		flex: none;
		width: 0.875rem;
		height: 0.875rem;
		border: 1px solid var(--color-border-strong);
	}

	.caption {
		margin: 0;
		color: var(--text-secondary);
		font-size: var(--caption);
	}
</style>
