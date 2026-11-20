<!--
	"Side by side": two flights in numbers, the one on screen and the one laid
	beside it, in columns, the number that is ahead in bold. Which is ahead
	and every word is the core's (`compare_flights`), as on the iPhone's
	`ComparisonSheet`.
-->
<script lang="ts">
	import Sheet from '#lib/design/Sheet.svelte';
	import type { Beside } from './beside';

	interface Props {
		open: boolean;
		beside: Beside;
		/** Marks the moment the two gliders were nearest, where they flew together. */
		onnearest: () => void;
		/** Takes the other flight away again. */
		onend: () => void;
		onclose: () => void;
	}

	let { open, beside, onnearest, onend, onclose }: Props = $props();
</script>

<Sheet {open} title="Side by side" {onclose}>
	<div class="side-by-side">
		<p class="words">{beside.comparison.words}</p>
		<table class="card numeric">
			<thead>
				<tr>
					<td></td>
					<th scope="col">This flight</th>
					<th scope="col" class="other">{beside.name}</th>
				</tr>
			</thead>
			<tbody>
				{#each beside.comparison.rows as row (row.label)}
					<tr>
						<th scope="row">{row.label}</th>
						<td class:ahead={row.lead === 'first'}>{row.first}</td>
						<td class="other" class:ahead={row.lead === 'other'}>{row.other}</td>
					</tr>
				{/each}
			</tbody>
		</table>
		<p class="note">
			The number that is ahead is set in bold. Worked out by Venti from the two recordings; where
			one of them is cut short, so are its numbers.
		</p>
		<div class="group-card actions">
			{#if beside.comparison.nearest}
				<button class="group-row" type="button" onclick={onnearest}>
					Go to where they were nearest
				</button>
			{/if}
			<button class="group-row destructive" type="button" onclick={onend}>
				Take the other flight away
			</button>
		</div>
	</div>
</Sheet>

<style>
	.side-by-side {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		padding: 0 var(--gutter) 1.5rem;
	}

	.words {
		color: var(--text-secondary);
	}

	table {
		width: 100%;
		border-collapse: collapse;
		background: var(--surface);
		font-size: var(--subheadline);
	}

	th,
	td {
		padding: 0.5rem 0.75rem;
		text-align: right;
		font-weight: 400;
	}

	th[scope='row'] {
		text-align: left;
		color: var(--text-secondary);
	}

	thead th {
		font-weight: 600;
		color: var(--text-secondary);
		max-width: 8rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	tbody tr {
		box-shadow: inset 0 1px 0 var(--separator);
	}

	/* The other flight's numbers carry its colour as a mark, not as text. */
	.other {
		box-shadow: inset 0.1875rem 0 0 var(--other);
	}

	.ahead {
		font-weight: 600;
	}

	.note {
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	.actions button {
		color: var(--link);
	}

	.actions button.destructive {
		color: var(--danger);
	}
</style>
