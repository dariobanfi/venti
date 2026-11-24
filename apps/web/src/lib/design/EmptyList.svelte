<!--
	What a list of the map's sheets says in its place when it has nothing to
	show: a title and a line under it, in the middle.
-->
<script lang="ts">
	let { title, line }: { title: string | undefined; line: string | undefined } = $props();
</script>

<div class="nothing">
	<h3>{title}</h3>
	<p>{line}</p>
</div>

<style>
	.nothing {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.375rem;
		padding: 3rem 1.5rem;
		text-align: center;
	}

	.nothing h3 {
		font-size: var(--title);
		font-weight: 600;
	}

	.nothing p {
		color: var(--text-secondary);
	}
</style>
