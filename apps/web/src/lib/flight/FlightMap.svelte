<!--
	The map of one flight: its track coloured by climb, where it started and
	ended, a marker at the moment the pilot is inspecting, and under it all
	the airspace the app knows there, as it is today.
-->
<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import { type AirspaceForMaps, type AirspaceOnMap, drawAirspace } from '#lib/airspace/on-map.ts';
	import type { FlightScore, FlightView, TrackPoint } from '#lib/core/index.ts';
	import {
		ACCENT_ON_MAP,
		CLIMB_BANDS,
		INK_ON_MAP,
		OTHER_ON_MAP,
		PAPER_ON_MAP,
		ROUTE_ON_MAP
	} from '#lib/design/palette.ts';
	import { addLine } from '#lib/map/casing.ts';
	import type { Map } from '#lib/map/setup.ts';
	import { NOTHING, place, routeLines, trackByBand } from '#lib/map/shapes.ts';

	interface Props {
		flight: FlightView;
		/** The moment to mark, if any. */
		cursor: TrackPoint | undefined;
		/** How the flight scores, once that has been worked out: its route is drawn. */
		score: FlightScore | undefined;
		/** How far the screen's bars and the panel reach in from the top and bottom, in pixels. */
		coveredTop: number;
		coveredBottom: number;
		/** Whether the map is tilted to show the terrain in 3D. */
		terrain3d: boolean;
		/** Whether the ground is photographs from above instead of the drawn map. */
		aerial: boolean;
		/** Where the airspace to draw comes from, once the app has it. */
		airspace: AirspaceForMaps | undefined;
		/** Called with how many airspaces are drawn, or nothing while none can be. */
		onairspace: (count: number | undefined) => void;
		/** A second flight laid beside this one: its track, and where its glider was at the marked moment. */
		other?: { track: readonly TrackPoint[]; at: TrackPoint | undefined };
	}

	let {
		flight,
		cursor,
		score,
		coveredTop,
		coveredBottom,
		terrain3d,
		aerial,
		airspace,
		onairspace,
		other
	}: Props = $props();

	let container: HTMLDivElement;
	let setup: typeof import('#lib/map/setup.ts') | undefined;
	/** The map, once its style is there to draw on. */
	let map = $state.raw<Map>();
	let tilted = false;
	let airspaceOnMap: AirspaceOnMap | undefined;

	onMount(() => {
		let gone = false;
		let created: Map | undefined;
		void import('#lib/map/setup.ts').then((module) => {
			if (gone) return;
			setup = module;
			const first = flight.track[0];
			created = module.createMap(container, 'Map of the flight', {
				center: first ? [first.lon, first.lat] : [11.6, 47.45],
				zoom: 10
			});
			const styled = created;
			module.whenStyled(styled, () => {
				// Airspace lies under everything of the flight.
				airspaceOnMap = drawAirspace(
					module,
					styled,
					() => airspace,
					(count) => onairspace(count)
				);
				addRoute(module, styled);
				// The other flight lies under this one, in one colour.
				styled.addSource('other-track', { type: 'geojson', data: NOTHING });
				addLine(styled, module.line('other-track-edge', 'other-track', INK_ON_MAP, 4.5));
				styled.addLayer(module.line('other-track-line', 'other-track', OTHER_ON_MAP, 2.5));
				addTrack(module, styled);
				addEnds(module, styled);
				styled.addSource('other-cursor', { type: 'geojson', data: NOTHING });
				styled.addLayer(
					module.dot('other-cursor-dot', 'other-cursor', 7, OTHER_ON_MAP, PAPER_ON_MAP)
				);
				styled.addSource('flight-cursor', { type: 'geojson', data: NOTHING });
				styled.addLayer(
					module.dot('flight-cursor-dot', 'flight-cursor', 8, ACCENT_ON_MAP, PAPER_ON_MAP)
				);
				map = styled;
			});
		});
		return () => {
			gone = true;
			created?.remove();
		};
	});

	/** The scored route goes under the track: a thin dashed line. */
	function addRoute(module: typeof import('#lib/map/setup.ts'), map: Map) {
		map.addSource('flight-route', { type: 'geojson', data: NOTHING });
		const route = module.line('flight-route-line', 'flight-route', ROUTE_ON_MAP, 1.5);
		addLine(map, { ...route, paint: { ...route.paint, 'line-dasharray': [4, 3] } });
	}

	function addTrack(module: typeof import('#lib/map/setup.ts'), map: Map) {
		map.addSource('flight-track', { type: 'geojson', data: trackByBand(flight.track) });
		// A dark casing keeps the track readable over any ground colour.
		map.addLayer(module.line('flight-track-edge', 'flight-track', INK_ON_MAP, 6.5));
		// Sink first and climbs last, so that where the track crosses itself
		// the thermals stay on top.
		CLIMB_BANDS.forEach((band, index) => {
			map.addLayer({
				...module.line(`flight-track-${index}`, 'flight-track', band.color, 3.5),
				filter: ['==', ['get', 'band'], index]
			});
		});
	}

	/** A hollow dot where the recording starts and a solid one where it ends. */
	function addEnds(module: typeof import('#lib/map/setup.ts'), map: Map) {
		const [first, last] = [flight.track[0], flight.track.at(-1)];
		if (!first || !last) return;
		map.addSource('flight-start', { type: 'geojson', data: place(first) });
		map.addLayer(module.dot('flight-start-dot', 'flight-start', 5, PAPER_ON_MAP, INK_ON_MAP));
		map.addSource('flight-end', { type: 'geojson', data: place(last) });
		map.addLayer(module.dot('flight-end-dot', 'flight-end', 5, INK_ON_MAP, PAPER_ON_MAP));
	}

	// Fits the whole track into the part of the map that shows, again
	// whenever that part changes size.
	$effect(() => {
		if (!map || !setup) return;
		setup.showBounds(map, flight.bounds, {
			top: coveredTop,
			right: 0,
			bottom: coveredBottom,
			left: 0
		});
	});

	$effect(() => {
		if (!map || !setup) return;
		setup.shapes(map, 'flight-route')?.setData(score ? routeLines(score) : NOTHING);
	});

	// Draws the airspace again when the app gets it, and when it changes.
	$effect(() => {
		if (!map || airspace?.revision === undefined) return;
		airspaceOnMap?.look();
	});

	$effect(() => {
		if (!map || !setup) return;
		setup.shapes(map, 'flight-cursor')?.setData(cursor ? place(cursor) : NOTHING);
	});

	/** Kept apart from the marked moment, so that moving the marker does not move the map. */
	const otherTrack = $derived(other?.track);
	const otherMarked = $derived(other?.at);

	$effect(() => {
		if (!map || !setup) return;
		setup.shapes(map, 'other-cursor')?.setData(otherMarked ? place(otherMarked) : NOTHING);
	});

	$effect(() => {
		if (!map || !setup) return;
		setup.shapes(map, 'other-track')?.setData(otherTrack ? trackLine(otherTrack) : NOTHING);
		// Room for both tracks.
		if (otherTrack && otherTrack.length > 1) {
			const lats = otherTrack.map((point) => point.lat);
			const lons = otherTrack.map((point) => point.lon);
			setup.showBounds(
				map,
				{
					south: Math.min(flight.bounds.south, ...lats),
					west: Math.min(flight.bounds.west, ...lons),
					north: Math.max(flight.bounds.north, ...lats),
					east: Math.max(flight.bounds.east, ...lons)
				},
				{ top: untrack(() => coveredTop), right: 0, bottom: untrack(() => coveredBottom), left: 0 }
			);
		}
	});

	/** A track as one line, without its colours. */
	function trackLine(track: readonly TrackPoint[]) {
		return {
			type: 'FeatureCollection' as const,
			features: [
				{
					type: 'Feature' as const,
					properties: {},
					geometry: {
						type: 'LineString' as const,
						coordinates: track.map((point) => [point.lon, point.lat])
					}
				}
			]
		};
	}

	$effect(() => {
		if (!map || !setup || terrain3d === tilted) return;
		tilted = terrain3d;
		setup.setTerrain3d(map, terrain3d);
	});

	$effect(() => {
		if (!map || !setup) return;
		setup.setAerial(map, aerial);
	});

	/** One step closer or further away, for the buttons of a wide screen. */
	export function zoomBy(steps: number) {
		map?.easeTo({ zoom: map.getZoom() + steps, duration: 250 });
	}
</script>

<div class="map" bind:this={container}></div>

<style>
	.map {
		position: absolute;
		inset: 0;
	}
</style>
