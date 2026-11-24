<!--
	"Filter" over a list of public flights, and what it holds: the glider's
	class, the kind of route, a least distance, the take-off's country and
	the area the take-off lies in (`AreaSheet.svelte`). Each choice that is
	on stands beside "Filter" as a chip, in the core's words, with a × that
	lets go of it. A page may add choices of its own: `chips` beside the
	others, `fields` first in the panel (the days of everyone's flights).

	What is chosen is the page's, in its address: this only says what the
	pilot changed.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';
	import { getApp } from '#lib/app/app.svelte.ts';
	import type {
		FlightSearch,
		GliderClassKind,
		RouteKind,
		SearchChip,
		SearchChoices
	} from '#lib/core/index.ts';
	import ChipTick from '#lib/design/ChipTick.svelte';
	import Icon from '#lib/design/Icon.svelte';
	import AreaSheet from './AreaSheet.svelte';
	import { countries } from './country';
	import { without } from './flight-search';

	interface Props {
		search: FlightSearch;
		onsearch: (search: FlightSearch) => void;
		/** Whether the panel is unfolded. */
		open?: boolean;
		/** Chips of the page's own, before the search's. */
		chips?: Snippet;
		/** Fields of the page's own, first in the panel. */
		fields?: Snippet;
	}

	let { search, onsearch, open = $bindable(false), chips, fields }: Props = $props();

	const app = getApp();

	let shown = $state<SearchChip[]>([]);
	$effect(() => {
		const asked = search;
		void app.everyone?.searchChips(asked).then(
			(words) => {
				if (asked === search) shown = words;
			},
			() => (shown = [])
		);
	});

	let choices = $state<SearchChoices>();
	$effect(() => {
		void app.everyone?.searchChoices().then((offered) => (choices = offered));
	});

	/** The classes as the core names them, without its "None". */
	const classes = $derived((app.said?.gliderClasses ?? []).filter((one) => one.kind !== undefined));
	const lands = countries();

	let picking = $state(false);

	function set(change: Partial<FlightSearch>): void {
		const next: FlightSearch = { ...search, ...change };
		for (const key of Object.keys(next) as (keyof FlightSearch)[]) {
			if (next[key] === undefined) delete next[key];
		}
		onsearch(next);
	}

	/** The least distance as typed: nothing, or nought, is none. */
	function least(typed: string): void {
		const km = Number(typed);
		const most = choices?.bounds.maxMinKm ?? 1000;
		set({ minKm: typed.trim() && Number.isFinite(km) && km > 0 ? Math.min(km, most) : undefined });
	}

	const id = $props.id();
</script>

<div class="filter">
	<button
		class="button-plain fold"
		type="button"
		aria-expanded={open}
		aria-controls="{id}-panel"
		onclick={() => (open = !open)}
	>
		<Icon name="filter" />
		{app.said?.pages.filter}
	</button>
	{@render chips?.()}
	{#each shown as chip (chip.part)}
		<span class="chip numeric">
			<ChipTick on />
			{chip.words}
			<button
				type="button"
				aria-label="Remove {chip.words}"
				title="Remove {chip.words}"
				onclick={() => onsearch(without(search, chip.part))}
			>
				<Icon name="close" />
			</button>
		</span>
	{/each}
</div>

{#if open}
	<div class="panel" id="{id}-panel" role="group" aria-label="Filter">
		{@render fields?.()}
		<div class="fields">
			<button class="button-plain area" type="button" onclick={() => (picking = true)}>
				<Icon name="map" />
				Area
			</button>
			<span class="choice">
				<label for="{id}-class">Glider</label>
				<select
					id="{id}-class"
					value={search.gliderClass ?? ''}
					onchange={(event) =>
						set({ gliderClass: (event.currentTarget.value || undefined) as GliderClassKind })}
				>
					<option value="">Any class</option>
					{#each classes as one (one.kind)}
						<option value={one.kind}>{one.name}</option>
					{/each}
				</select>
			</span>
			<span class="choice">
				<label for="{id}-kind">Kind</label>
				<select
					id="{id}-kind"
					value={search.kind ?? ''}
					onchange={(event) => set({ kind: (event.currentTarget.value || undefined) as RouteKind })}
				>
					<option value="">Any kind</option>
					{#each choices?.kinds ?? [] as one (one.kind)}
						<option value={one.kind}>{one.name}</option>
					{/each}
				</select>
			</span>
			<span class="choice">
				<label for="{id}-least">At least</label>
				<input
					id="{id}-least"
					class="numeric"
					type="number"
					inputmode="decimal"
					min="0"
					max={choices?.bounds.maxMinKm ?? 1000}
					step="1"
					value={search.minKm ?? ''}
					onchange={(event) => least(event.currentTarget.value)}
				/>
				<span>km</span>
			</span>
			<span class="choice">
				<label for="{id}-country">Take-off in</label>
				<select
					id="{id}-country"
					value={search.country ?? ''}
					onchange={(event) => set({ country: event.currentTarget.value || undefined })}
				>
					<option value="">Worldwide</option>
					{#each lands as land (land.code)}
						<option value={land.code}>{land.name}</option>
					{/each}
				</select>
			</span>
		</div>
	</div>
{/if}

<AreaSheet
	open={picking}
	{search}
	bounds={choices?.bounds}
	onuse={(area) => {
		picking = false;
		const rest = without(without(search, 'near'), 'area');
		onsearch({ ...rest, ...area });
	}}
	onclose={() => (picking = false)}
/>

<style>
	/* "Filter", and the chips of what is chosen beside it. */
	.filter {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
	}

	.fold,
	.area {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
	}

	.fold :global(svg),
	.area :global(svg) {
		width: 1.125rem;
		height: 1.125rem;
	}

	/* The pack's chip, chosen: what the list is held to. */
	.chip {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		min-height: var(--space-6);
		padding: 0 var(--space-1) 0 var(--space-3);
		border-radius: var(--radius-pill);
		background: var(--accent);
		color: var(--on-accent);
		font-size: var(--subheadline);
		font-weight: 500;
	}

	.chip button {
		display: grid;
		place-items: center;
		width: 1.75rem;
		height: 1.75rem;
		border-radius: 50%;
		color: inherit;
	}

	.chip :global(.tick) {
		margin-inline-end: 0;
	}

	.chip button :global(svg) {
		width: 1rem;
		height: 1rem;
	}

	/* A row of its own under the others. */
	.panel {
		flex-basis: 100%;
		order: 1;
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		font-size: var(--subheadline);
	}

	.fields {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2) var(--space-4);
	}

	.area {
		margin-inline-start: calc(-1 * var(--space-3));
	}

	/* A field and its name stay together when the row wraps; on a phone
	   each is a row of its own, the names in a column. */
	.choice {
		display: flex;
		flex-basis: 100%;
		align-items: center;
		gap: var(--space-2);
		color: var(--text-secondary);
	}

	.choice label {
		flex: none;
		min-width: 6.5rem;
	}

	@media (min-width: 40rem) {
		.choice {
			display: inline-flex;
			flex-basis: auto;
		}

		.choice label {
			min-width: 0;
		}
	}

	select,
	input {
		min-height: 2.25rem;
		padding: 0 var(--space-2);
		border: 1.5px solid var(--color-border-strong);
		border-radius: var(--radius-control);
		background: var(--surface);
		color: var(--text);
		font: inherit;
	}

	select {
		max-width: 12rem;
	}

	input {
		width: 5rem;
	}

	@media (min-width: 56rem) {
		select,
		input {
			min-height: var(--target);
		}
	}
</style>
