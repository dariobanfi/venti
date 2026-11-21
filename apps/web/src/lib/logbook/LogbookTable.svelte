<!--
	The logbook on a wide screen: every flight in one table, with headings
	that sort, a year to narrow it to, a search, and a box on each row to
	choose several flights for downloading or deleting at once. A row opens
	its flight. Beside a flight the same table stands narrower, with the
	flight being looked at marked.

	The figures are the core's; `table.ts` orders and picks among them.
-->
<script lang="ts">
	import { goto } from '$app/navigation';
	import { flightHref } from '#lib/app/routes.ts';
	import type { LogbookEntry, LogbookView } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import { distanceOf, NEWEST_FIRST, type Sort, type SortKey, sortBy, tableRows } from './table';

	interface Props {
		view: LogbookView;
		/** The flight on screen beside the table, marked in it. */
		current?: string;
		/** Beside a flight: fewer columns, and no choosing of several. */
		compact?: boolean;
		ondelete?: (entries: LogbookEntry[]) => void;
		ondownload?: (entries: LogbookEntry[]) => void;
	}

	let { view, current, compact = false, ondelete, ondownload }: Props = $props();

	const text = format();

	let sort = $state<Sort>(NEWEST_FIRST);
	let year = $state<number>();
	let search = $state('');
	/** The flights chosen with their boxes, by identifier. */
	let chosen = $state<string[]>([]);

	const rows = $derived(tableRows(view, year, search, sort));
	const chosenRows = $derived(rows.filter((entry) => chosen.includes(entry.id)));
	const allChosen = $derived(rows.length > 0 && chosenRows.length === rows.length);

	const columns = $derived<{ key: SortKey; name: string; numeric: boolean }[]>(
		compact
			? [
					{ key: 'date', name: 'Date', numeric: false },
					{ key: 'place', name: 'Take-off', numeric: false },
					{ key: 'airtime', name: 'Airtime', numeric: true },
					{ key: 'distance', name: 'km', numeric: true }
				]
			: [
					{ key: 'date', name: 'Date', numeric: false },
					{ key: 'place', name: 'Take-off', numeric: false },
					{ key: 'airtime', name: 'Airtime', numeric: true },
					{ key: 'distance', name: 'Distance', numeric: true },
					{ key: 'points', name: 'Points', numeric: true },
					{ key: 'highest', name: 'Highest', numeric: true },
					{ key: 'climb', name: 'Best climb', numeric: true }
				]
	);

	function toggle(id: string) {
		chosen = chosen.includes(id) ? chosen.filter((one) => one !== id) : [...chosen, id];
	}

	function toggleAll() {
		chosen = allChosen ? [] : rows.map((entry) => entry.id);
	}

	/** A click anywhere on a row opens its flight; the box and the link do their own. */
	function open(event: MouseEvent, entry: LogbookEntry) {
		const target = event.target as HTMLElement;
		if (target.closest('a, input, button')) return;
		void goto(flightHref(entry.id));
	}

	function ariaSort(key: SortKey): 'ascending' | 'descending' | undefined {
		if (sort.key !== key) return undefined;
		return sort.descending ? 'descending' : 'ascending';
	}

	function shape(entry: LogbookEntry): string {
		if (entry.score?.kind === 'faiTriangle') return 'FAI triangle';
		if (entry.score?.kind === 'freeTriangle') return 'Triangle';
		return '';
	}
</script>

<div class="table" class:compact>
	<div class="tools">
		<input
			class="search"
			type="search"
			placeholder="Take-off, glider"
			aria-label="Search flights"
			bind:value={search}
		/>
		<select aria-label="Year" bind:value={year}>
			<option value={undefined}>All years</option>
			{#each view.years as one (one.year)}
				<option value={one.year}>{one.year}</option>
			{/each}
		</select>
		{#if !compact}
			<span class="count numeric" role="status">
				{#if chosenRows.length > 0}
					{chosenRows.length} of {text.flights(rows.length)} chosen
				{:else}
					{text.flights(rows.length)}
				{/if}
			</span>
			{#if chosenRows.length > 0}
				<button class="tool" type="button" onclick={() => ondownload?.(chosenRows)}>
					<Icon name="download" />
					{chosenRows.length === 1 ? 'Download file' : `Download ${chosenRows.length} files`}
				</button>
				<button class="tool destructive" type="button" onclick={() => ondelete?.(chosenRows)}>
					<Icon name="trash" />
					Delete
				</button>
			{/if}
		{/if}
	</div>

	<div class="scroller">
		<table aria-label="Flights">
			<thead>
				<tr>
					{#if !compact}
						<th class="box">
							<input
								type="checkbox"
								aria-label="Choose every flight listed"
								checked={allChosen}
								indeterminate={chosenRows.length > 0 && !allChosen}
								onchange={toggleAll}
							/>
						</th>
					{/if}
					{#each columns as column (column.key)}
						<th class:numeric={column.numeric} aria-sort={ariaSort(column.key)} scope="col">
							<button type="button" onclick={() => (sort = sortBy(sort, column.key))}>
								{column.name}
								{#if sort.key === column.key}
									<span class="arrow" class:up={!sort.descending} aria-hidden="true">
										<Icon name="forward" />
									</span>
								{/if}
							</button>
						</th>
					{/each}
					{#if !compact}
						<th scope="col">Glider</th>
					{/if}
				</tr>
			</thead>
			<tbody>
				{#each rows as entry (entry.id)}
					<tr
						class:current={entry.id === current}
						class:chosen={chosen.includes(entry.id)}
						onclick={(event) => open(event, entry)}
					>
						{#if !compact}
							<td class="box">
								<input
									type="checkbox"
									aria-label="Choose the flight of {text.day(entry.startUnixS)}"
									checked={chosen.includes(entry.id)}
									onchange={() => toggle(entry.id)}
								/>
							</td>
						{/if}
						<td class="date">
							<a
								href={flightHref(entry.id)}
								aria-current={entry.id === current ? 'page' : undefined}
							>
								{compact
									? text.dayInYear(entry.startUnixS)
									: `${text.day(entry.startUnixS)} ${text.time(entry.startUnixS)}`}
							</a>
						</td>
						<td class="place">{entry.takeoffName ?? '–'}</td>
						<td class="numeric">{text.hoursMinutes(entry.durationS)}</td>
						<td class="numeric">
							{#if shape(entry)}
								<span class="shape" role="img" aria-label={shape(entry)}>
									<Icon name="triangle" />
								</span>
							{/if}
							{text.kilometres(distanceOf(entry))}{compact ? '' : ' km'}
						</td>
						{#if !compact}
							<td class="numeric">{entry.score ? text.points(entry.score.points) : '–'}</td>
							<td class="numeric">{text.metres(entry.maxAltitudeM)} m</td>
							<td class="numeric">{text.climb(entry.maxClimbMs)} m/s</td>
							<td class="place">{entry.glider ?? ''}</td>
						{/if}
					</tr>
				{:else}
					<tr class="none">
						<td colspan={columns.length + (compact ? 0 : 2)}>No flight answers this search.</td>
					</tr>
				{/each}
			</tbody>
		</table>
	</div>
</div>

<style>
	.table {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		min-height: 0;
	}

	.tools {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
	}

	.search {
		flex: 1 1 12rem;
		min-width: 8rem;
	}

	.search,
	select,
	.tool {
		height: var(--target);
		padding: 0 var(--space-2);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-s);
		background: var(--surface);
		color: var(--text);
		font: inherit;
	}

	.compact .search {
		flex-basis: 6rem;
	}

	.count {
		color: var(--text-secondary);
		margin-right: auto;
	}

	.tool {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		--icon-size: 1rem;
	}

	.tool :global(svg) {
		width: 1rem;
		height: 1rem;
	}

	.destructive {
		color: var(--color-danger);
	}

	.scroller {
		overflow: auto;
		min-height: 0;
		background: var(--surface);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-m);
	}

	table {
		width: 100%;
		border-collapse: collapse;
	}

	th,
	td {
		height: 2rem;
		padding: 0 var(--space-2);
		text-align: left;
		white-space: nowrap;
	}

	th {
		position: sticky;
		top: 0;
		z-index: 1;
		background: var(--surface);
		box-shadow: inset 0 -1px 0 var(--color-border);
		font-size: var(--subheadline);
		font-weight: 600;
		color: var(--text-secondary);
	}

	th button {
		display: inline-flex;
		align-items: center;
		gap: 0.125rem;
		font: inherit;
		color: inherit;
	}

	.numeric {
		text-align: right;
		font-variant-numeric: tabular-nums;
	}

	th.numeric button {
		flex-direction: row-reverse;
	}

	.arrow {
		display: inline-flex;
	}

	.arrow :global(svg) {
		width: 0.875rem;
		height: 0.875rem;
	}

	/* The pack's forward chevron, turned to point down, or up for the smallest first. */
	.arrow {
		transform: rotate(90deg);
	}

	.arrow.up {
		transform: rotate(-90deg);
	}

	tbody tr {
		cursor: pointer;
		box-shadow: inset 0 -1px 0 var(--separator);
	}

	.place {
		max-width: 14rem;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.compact .place {
		max-width: 8rem;
	}

	.date {
		font-variant-numeric: tabular-nums;
	}

	.date a {
		color: var(--text);
		font-weight: 500;
	}

	.box {
		width: 2rem;
		padding-right: 0;
	}

	.shape {
		display: inline-flex;
		vertical-align: -0.125rem;
		color: var(--text-secondary);
	}

	.shape :global(svg) {
		width: 0.875rem;
		height: 0.875rem;
	}

	tr.chosen {
		background: var(--fill);
	}

	tr.current {
		background: var(--fill);
		box-shadow: inset 3px 0 0 var(--color-primary);
	}

	.none td {
		color: var(--text-secondary);
		cursor: default;
	}

	@media (hover: hover) {
		tbody tr:not(.none):hover {
			background: var(--fill);
		}
	}
</style>
