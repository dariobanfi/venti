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
		padding: 0.125rem;
		border-radius: 0.625rem;
		background: var(--fill);
	}

	a {
		flex: 1;
		display: grid;
		place-items: center;
		min-height: 2rem;
		border-radius: 0.5rem;
		font-size: var(--subheadline);
	}

	a[aria-current] {
		background: var(--surface);
		font-weight: 600;
		box-shadow: 0 1px 3px rgb(0 0 0 / 0.14);
	}

	@media (prefers-color-scheme: dark) {
		a[aria-current] {
			background: var(--fill-strong);
		}
	}
</style>
