<!-- What the colours of the track mean, in one line. -->
<script lang="ts">
	import { CLIMB_BANDS } from '#lib/design/palette.ts';
</script>

<p
	class="legend"
	role="img"
	aria-label="Track colours: blue where the glider sank, grey where it held its height, orange and red where it climbed."
>
	<span>Sink</span>
	<span class="bands">
		{#each CLIMB_BANDS as band (band.color)}
			<span class="band" style:background={band.color}></span>
		{/each}
	</span>
	<span>Climb</span>
</p>

<style>
	.legend {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		font-size: var(--caption);
		color: var(--metric-secondary, var(--text-secondary));
	}

	.bands {
		flex: 1;
		display: flex;
		gap: 0.125rem;
	}

	.band {
		flex: 1;
		height: 0.375rem;
		border-radius: 999px;
	}
</style>
