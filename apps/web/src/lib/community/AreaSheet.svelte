<!--
	Where the flights searched for took off: a mark and a radius around it,
	or an area drawn on the map. "Use this area" hands the choice back; the
	core says why one cannot be asked (an area whose edges cross), and the
	server's bounds are the core's (`flightSearchBounds`).
-->
<script lang="ts">
	import { getApp } from '#lib/app/app.svelte.ts';
	import type { Coordinate, FlightSearch, FlightSearchBounds } from '#lib/core/index.ts';
	import Icon from '#lib/design/Icon.svelte';
	import Segmented from '#lib/design/Segmented.svelte';
	import Sheet from '#lib/design/Sheet.svelte';
	import AreaMap from './AreaMap.svelte';

	interface Props {
		open: boolean;
		/** The search as it stands, whose area the sheet opens on. */
		search: FlightSearch;
		bounds: FlightSearchBounds | undefined;
		/** Told the area chosen: a circle, or the corners of a drawn one. */
		onuse: (area: Pick<FlightSearch, 'near' | 'radiusKm' | 'area'>) => void;
		onclose: () => void;
	}

	let { open, search, bounds, onuse, onclose }: Props = $props();

	const app = getApp();

	const RADIUS = 'Radius';
	const DRAW = 'Draw an area';

	/** The middle of the Alps north and south of the border, where a new circle starts. */
	const MIDDLE: Coordinate = { lat: 47.45, lon: 11.6 };

	let mode = $state(RADIUS);
	let centre = $state<Coordinate>(MIDDLE);
	let radiusKm = $state(10);
	let corners = $state<Coordinate[]>([]);
	let closed = $state(false);
	let frame = $state(false);
	let refused = $state<string>();

	const smallest = $derived(bounds?.minRadiusKm ?? 1);
	const largest = $derived(bounds?.maxRadiusKm ?? 100);
	const most = $derived(bounds?.maxAreaPoints ?? 50);

	// Each time the sheet opens it starts from the search as it stands.
	let wasOpen = false;
	$effect(() => {
		if (open && !wasOpen) {
			const drawn = search.area ?? [];
			mode = drawn.length > 0 ? DRAW : RADIUS;
			centre = search.near ?? MIDDLE;
			radiusKm = search.radiusKm ?? 10;
			corners = [...drawn];
			closed = drawn.length > 0;
			frame = Boolean(search.near) || drawn.length > 0;
			refused = undefined;
		}
		wasOpen = open;
	});

	function clamp(km: number): number {
		return Math.min(largest, Math.max(smallest, Math.round(km)));
	}

	const drawing = $derived(mode === DRAW);
	const ready = $derived(drawing ? corners.length >= 3 : true);

	async function use(): Promise<void> {
		const area = drawing
			? { area: corners.map((corner) => ({ ...corner })) }
			: { near: { ...centre }, radiusKm };
		refused = await app.everyone?.searchRefused(area);
		if (refused) return;
		onuse(area);
	}

	const id = $props.id();
</script>

<Sheet {open} title="Area" {onclose}>
	<div class="area">
		<div class="mode">
			<Segmented
				label="Area"
				options={[RADIUS, DRAW]}
				bind:value={() => mode, (chosen) => ((mode = chosen), (refused = undefined))}
			/>
		</div>
		{#if open}
			<AreaMap
				mode={drawing ? 'draw' : 'radius'}
				{centre}
				{radiusKm}
				{corners}
				{closed}
				{frame}
				maxCorners={most}
				label={drawing ? 'Map: tap the corners of the area' : 'Map: the place and the radius'}
				onplace={(at) => (centre = at)}
				onradius={(km) => (radiusKm = clamp(km))}
				oncorner={(at) => {
					corners = [...corners, at];
					refused = undefined;
				}}
				onclose={() => (closed = true)}
			/>
		{/if}
		{#if drawing}
			<p class="line numeric" role="status">
				{#if closed}
					{corners.length} corners
				{:else if corners.length === 0}
					Tap the corners on the map.
				{:else if corners.length < most}
					{corners.length} of at most {most} corners
				{:else}
					{most} corners, the most an area has
				{/if}
			</p>
			<div class="row">
				<button
					class="button-plain"
					type="button"
					disabled={corners.length === 0}
					onclick={() => {
						corners = corners.slice(0, -1);
						closed = false;
						refused = undefined;
					}}
				>
					Undo
				</button>
				<button
					class="button-plain"
					type="button"
					disabled={corners.length === 0}
					onclick={() => {
						corners = [];
						closed = false;
						refused = undefined;
					}}
				>
					Start again
				</button>
				<button
					class="button-plain"
					type="button"
					disabled={corners.length < 3 || closed}
					onclick={() => (closed = true)}
				>
					Close the shape
				</button>
			</div>
		{:else}
			<div class="radius">
				<label for="{id}-radius">Radius</label>
				<input
					id="{id}-radius"
					type="range"
					min={smallest}
					max={largest}
					step="1"
					bind:value={radiusKm}
				/>
				<output class="numeric" for="{id}-radius">{radiusKm} km</output>
			</div>
		{/if}
		{#if refused}
			<p class="refused" role="alert"><Icon name="warning" /> {refused}</p>
		{/if}
		<button class="button-prominent use" type="button" disabled={!ready} onclick={use}>
			Use this area
		</button>
	</div>
</Sheet>

<style>
	.area {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		padding: 0 var(--space-4) var(--space-4);
	}

	.mode {
		max-width: 20rem;
	}

	.line {
		font-size: var(--subheadline);
		color: var(--text-secondary);
	}

	.row {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-1);
		margin-inline-start: calc(-1 * var(--space-3));
	}

	.radius {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		min-height: var(--target);
	}

	.radius label {
		color: var(--text-secondary);
	}

	.radius input {
		flex: 1;
		min-width: 0;
		min-height: var(--target);
		accent-color: var(--accent);
	}

	.radius output {
		min-width: 4.5rem;
		text-align: end;
		font-weight: 600;
	}

	.refused {
		display: flex;
		gap: var(--space-2);
		font-size: var(--subheadline);
		color: var(--danger);
	}

	.refused :global(svg) {
		flex: none;
		width: 1.125rem;
		height: 1.125rem;
	}

	.use {
		align-self: stretch;
	}
</style>
