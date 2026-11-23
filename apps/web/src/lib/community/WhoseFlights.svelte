<!--
	The switch at the head of the flights: the pilot's own flights on the
	account, or the flights everyone on the server shares. The owner asked
	for one that cannot be missed (2026-11-23): the pack's segmented control
	at the head of the page, as wide as a phone and at a finger's height,
	its words at the body's size. On the iPhone it is a segmented control in
	the flights' title bar; here they are pages, so it is links drawn as one
	control. The ranking is a section of its own in the bar.
-->
<script lang="ts">
	import { everyoneHref, myFlightsHref } from '#lib/app/routes.ts';

	interface Props {
		current: 'mine' | 'everyone';
		/** What was last asked of everyone's flights, to come back to it. */
		query?: string;
	}

	let { current, query = '' }: Props = $props();
</script>

<nav class="segments" aria-label="Whose flights">
	<a href={myFlightsHref()} aria-current={current === 'mine' ? 'page' : undefined}>Mine</a>
	<a href={everyoneHref(query)} aria-current={current === 'everyone' ? 'page' : undefined}>
		Everyone
	</a>
</nav>

<style>
	.segments {
		display: flex;
		gap: var(--space-1);
		width: 100%;
		max-width: 28rem;
		padding: var(--space-1);
		border-radius: var(--radius-pill);
		background: var(--fill);
	}

	a {
		flex: 1;
		display: grid;
		place-items: center;
		min-height: var(--target);
		border-radius: var(--radius-pill);
		font-size: var(--body);
		font-weight: 600;
		color: var(--text-secondary);
	}

	/* The pack's segmented control: the chosen one in the primary colour. */
	a[aria-current] {
		background: var(--accent);
		color: var(--on-accent);
	}

	@media (min-width: 56rem) {
		.segments {
			max-width: 22rem;
		}
	}
</style>
