<!--
	The map of a search's area: a mark and the circle around it, or the
	corners of an area drawn on it. In "radius" the mark is dragged, or the
	map tapped where it goes, and the handle on the circle's edge drags the
	radius; in "draw" each tap is a corner, and a tap on the first corner
	closes the shape. The map is the app's, with its terrain.

	The sheet around it holds what is chosen; this only draws it and tells
	what the pilot did. The map engine is loaded when the sheet opens.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import type { Coordinate } from '#lib/core/index.ts';
	import type { Map } from '#lib/map/setup.ts';
	import type { GeoJSONSource, Marker } from 'maplibre-gl';
	import { ACCENT_ON_MAP, INK_ON_MAP, PAPER_ON_MAP } from '#lib/design/palette.ts';
	import { circleRing, kilometresBetween, placeFrom } from './flight-search';

	interface Props {
		mode: 'radius' | 'draw';
		/** The middle of the circle. */
		centre: Coordinate;
		radiusKm: number;
		/** The corners drawn so far, in the order they were drawn. */
		corners: Coordinate[];
		/** Whether the shape was closed on its first corner. */
		closed: boolean;
		/** The most corners an area may have. */
		maxCorners: number;
		/** Whether the map opens on what was chosen before, rather than on the Alps. */
		frame: boolean;
		/** What a screen reader calls the map. */
		label: string;
		onplace: (centre: Coordinate) => void;
		/** Told the distance of the edge's handle from the middle, kilometres. */
		onradius: (km: number) => void;
		oncorner: (corner: Coordinate) => void;
		onclose: () => void;
	}

	let {
		mode,
		centre,
		radiusKm,
		corners,
		closed,
		maxCorners,
		frame,
		label,
		onplace,
		onradius,
		oncorner,
		onclose
	}: Props = $props();

	/** Where the map opens without a search: the Alps north and south of the border, as the map tab. */
	const START = { center: [11.6, 47.45] as [number, number], zoom: 7.4 };
	/** How near the first corner a tap closes the shape, pixels. */
	const CLOSING_PX = 18;

	let container: HTMLDivElement;
	let map = $state.raw<Map>();
	let styled = $state(false);
	/** Whether the map has drawn what it holds once: busy until then. */
	let drawn = $state(false);
	let mark: Marker | undefined;
	let edge: Marker | undefined;
	let sizing: ResizeObserver | undefined;
	/** Whether the edge's handle is being dragged, which then stays under the finger. */
	let dragging = false;

	const NOTHING = { type: 'FeatureCollection' as const, features: [] };

	onMount(() => {
		let gone = false;
		void Promise.all([import('#lib/map/setup.ts'), import('maplibre-gl')]).then(
			([setup, engine]) => {
				if (gone) return;
				const shown = setup.createMap(container, label, START);
				// What was chosen before is in view when the sheet opens again.
				if (frame) {
					const points =
						mode === 'draw'
							? corners
							: circleRing(centre, radiusKm, 16).map(([lon, lat]) => ({ lat, lon }));
					const lons = points.map((point) => point.lon);
					const lats = points.map((point) => point.lat);
					if (points.length > 0) {
						shown.fitBounds(
							[
								[Math.min(...lons), Math.min(...lats)],
								[Math.max(...lons), Math.max(...lats)]
							],
							{ padding: 32, animate: false, maxZoom: 13 }
						);
					}
				}
				setup.whenStyled(shown, () => {
					shown.addSource('search-circle', { type: 'geojson', data: NOTHING });
					shown.addSource('search-area', { type: 'geojson', data: NOTHING });
					shown.addSource('search-corners', { type: 'geojson', data: NOTHING });
					for (const source of ['search-circle', 'search-area']) {
						shown.addLayer({
							id: `${source}-fill`,
							type: 'fill',
							source,
							filter: ['==', ['geometry-type'], 'Polygon'],
							paint: { 'fill-color': ACCENT_ON_MAP, 'fill-opacity': 0.14 }
						});
						shown.addLayer({
							...setup.line(`${source}-edge`, source, ACCENT_ON_MAP, 2.5)
						});
					}
					shown.addLayer(
						setup.dot('search-corner-dots', 'search-corners', 5, PAPER_ON_MAP, INK_ON_MAP)
					);
					styled = true;
				});
				shown.once('idle', () => (drawn = true));
				shown.on('click', (event) => {
					const at = { lat: event.lngLat.lat, lon: event.lngLat.lng };
					if (mode === 'radius') {
						onplace(at);
						return;
					}
					if (closed) return;
					const first = corners[0];
					if (first && corners.length >= 3) {
						const there = shown.project([first.lon, first.lat]);
						if (Math.hypot(there.x - event.point.x, there.y - event.point.y) <= CLOSING_PX) {
							onclose();
							return;
						}
					}
					if (corners.length < maxCorners) oncorner(at);
				});

				const ringed = document.createElement('div');
				ringed.className = 'area-mark';
				ringed.setAttribute('aria-hidden', 'true');
				mark = new engine.Marker({ element: ringed, draggable: true })
					.setLngLat([centre.lon, centre.lat])
					.addTo(shown);
				mark.on('dragend', () => {
					const at = mark?.getLngLat();
					if (at) onplace({ lat: at.lat, lon: at.lng });
				});
				const handle = document.createElement('div');
				handle.className = 'area-edge';
				handle.setAttribute('aria-hidden', 'true');
				edge = new engine.Marker({ element: handle, draggable: true })
					.setLngLat([centre.lon, centre.lat])
					.addTo(shown);
				edge.on('dragstart', () => (dragging = true));
				edge.on('drag', () => {
					const at = edge?.getLngLat();
					if (at) onradius(kilometresBetween(centre, { lat: at.lat, lon: at.lng }));
				});
				edge.on('dragend', () => {
					dragging = false;
					placeEdge();
				});
				map = shown;
				// The sheet is still taking its size as the map is made: the map
				// follows its box, not only the window.
				sizing = new ResizeObserver(() => shown.resize());
				sizing.observe(container);
			}
		);
		return () => {
			gone = true;
			sizing?.disconnect();
			map?.remove();
		};
	});

	/** Puts the edge's handle back on the circle, east of the middle. */
	function placeEdge(): void {
		const east = placeFrom(centre, radiusKm, 90);
		edge?.setLngLat([east.lon, east.lat]);
	}

	// The mark, the circle and its handle follow the choice.
	$effect(() => {
		if (!map || !styled) return;
		const radius = mode === 'radius';
		mark?.setLngLat([centre.lon, centre.lat]);
		mark?.getElement().toggleAttribute('hidden', !radius);
		edge?.getElement().toggleAttribute('hidden', !radius);
		if (!dragging) placeEdge();
		(map.getSource('search-circle') as GeoJSONSource | undefined)?.setData(
			radius
				? {
						type: 'Feature',
						properties: {},
						geometry: { type: 'Polygon', coordinates: [circleRing(centre, radiusKm)] }
					}
				: NOTHING
		);
	});

	// The corners drawn, the line through them and, once there are three, the shape.
	$effect(() => {
		if (!map || !styled) return;
		const drawing = mode === 'draw';
		const line = corners.map((corner) => [corner.lon, corner.lat]);
		const shape =
			drawing && corners.length >= 3
				? {
						type: 'Feature' as const,
						properties: {},
						geometry: { type: 'Polygon' as const, coordinates: [[...line, line[0]]] }
					}
				: drawing && corners.length === 2
					? {
							type: 'Feature' as const,
							properties: {},
							geometry: { type: 'LineString' as const, coordinates: line }
						}
					: NOTHING;
		(map.getSource('search-area') as GeoJSONSource | undefined)?.setData(shape);
		(map.getSource('search-corners') as GeoJSONSource | undefined)?.setData(
			drawing && !closed
				? {
						type: 'FeatureCollection',
						features: corners.map((corner) => ({
							type: 'Feature' as const,
							properties: {},
							geometry: { type: 'Point' as const, coordinates: [corner.lon, corner.lat] }
						}))
					}
				: NOTHING
		);
	});
</script>

<div class="map" bind:this={container} aria-busy={!drawn}></div>

<style>
	.map {
		position: relative;
		height: min(24rem, 55dvh);
		overflow: hidden;
		border-radius: var(--radius-card);
	}

	/* The mark of the middle: a ring around a point, in the ink of what is
	   written on the map, as the map tab's. 48 px to take a finger. */
	.map :global(.area-mark) {
		width: var(--target);
		height: var(--target);
		border: 0.1875rem solid var(--on-map-ink);
		border-radius: 50%;
		box-shadow:
			0 0 0 0.0625rem var(--on-map-paper),
			inset 0 0 0 0.0625rem var(--on-map-paper);
		cursor: grab;
	}

	.map :global(.area-mark::after) {
		content: '';
		position: absolute;
		top: 50%;
		left: 50%;
		width: 0.375rem;
		height: 0.375rem;
		margin: -0.1875rem 0 0 -0.1875rem;
		border-radius: 50%;
		background: var(--on-map-ink);
	}

	/* The handle on the circle's edge: a dot inside a target a finger takes. */
	.map :global(.area-edge) {
		display: grid;
		place-items: center;
		width: var(--target);
		height: var(--target);
		cursor: ew-resize;
	}

	.map :global(.area-edge::after) {
		content: '';
		width: 1rem;
		height: 1rem;
		border: 0.125rem solid var(--on-map-ink);
		border-radius: 50%;
		background: var(--on-map-paper);
	}

	.map :global([hidden]) {
		display: none;
	}
</style>
