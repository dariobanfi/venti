<!--
	The switch at the head of the logbook: the pilot's own flights, the
	flights everyone on the server shares, or the ranking of the pilots who
	shared them. On the iPhone it is a segmented control in the logbook's
	title bar; here they are pages, so it is links drawn as one control.
-->
<script lang="ts">
	import { everyoneHref, logbookHref, rankingHref } from '#lib/app/routes.ts';

	interface Props {
		current: 'mine' | 'everyone' | 'ranking';
		/** What was last asked of everyone's flights, to come back to it. */
		query?: string;
	}

	let { current, query = '' }: Props = $props();
</script>

<nav class="segments" aria-label="Whose flights">
	<a href={logbookHref()} aria-current={current === 'mine' ? 'page' : undefined}>Mine</a>
	<a href={everyoneHref(query)} aria-current={current === 'everyone' ? 'page' : undefined}>
		Everyone
	</a>
	<a href={rankingHref()} aria-current={current === 'ranking' ? 'page' : undefined}>Ranking</a>
</nav>

<style>
	.segments {
		display: flex;
		gap: 0.125rem;
		width: 21rem;
		max-width: 100%;
		padding: 0.1875rem;
		border-radius: var(--radius-pill);
		background: var(--fill);
	}

	a {
		flex: 1;
		display: grid;
		place-items: center;
		/* 32 px in all on a wide screen, as its other controls. */
		min-height: calc(var(--target) - 0.625rem);
		border-radius: var(--radius-pill);
		font-size: var(--subheadline);
		font-weight: 500;
		color: var(--text-secondary);
	}

	/* The pack's segmented control: the chosen one in the primary colour. */
	a[aria-current] {
		background: var(--accent);
		color: var(--on-accent);
	}
</style>
