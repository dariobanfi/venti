<!--
	"Draft": a post the owner has not finished, shown as such until its
	`draft: true` is taken out.
-->
<script lang="ts">
	// Nothing to set: the word and its look.
</script>

<span class="draft">Draft</span>

<style>
	.draft {
		display: inline-flex;
		align-items: center;
		padding: 0 var(--space-2);
		border: 1px solid var(--color-caution);
		border-radius: var(--radius-s);
		background: var(--color-caution-soft);
		color: var(--text);
		font-size: var(--caption);
		font-weight: 600;
		line-height: 1.5;
		vertical-align: 0.1em;
	}
</style>
