<!--
	The map of one flight: its track coloured by climb, where it started and
	ended, and a marker at the moment the pilot is inspecting.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import type { FlightScore, FlightView, TrackPoint } from '#lib/core/index.ts';
	import { ACCENT_ON_MAP, CLIMB_BANDS, INK_ON_MAP, ROUTE_ON_MAP } from '#lib/design/palette.ts';
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
	}

	let { flight, cursor, score, coveredTop, coveredBottom, terrain3d }: Props = $props();

	let container: HTMLDivElement;
	let setup: typeof import('#lib/map/setup.ts') | undefined;
	/** The map, once its style is there to draw on. */
	let map = $state.raw<Map>();
	let tilted = false;

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
				addRoute(module, styled);
				addTrack(module, styled);
				addEnds(module, styled);
				styled.addSource('flight-cursor', { type: 'geojson', data: NOTHING });
				styled.addLayer(
					module.dot('flight-cursor-dot', 'flight-cursor', 8, ACCENT_ON_MAP, '#ffffff')
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
		map.addLayer({ ...route, paint: { ...route.paint, 'line-dasharray': [4, 3] } });
	}

	function addTrack(module: typeof import('#lib/map/setup.ts'), map: Map) {
		map.addSource('flight-track', { type: 'geojson', data: trackByBand(flight.track) });
		// A white edge keeps the track readable over any ground colour.
		map.addLayer(module.line('flight-track-edge', 'flight-track', '#ffffff', 6.5));
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
		map.addLayer(module.dot('flight-start-dot', 'flight-start', 5, '#ffffff', INK_ON_MAP));
		map.addSource('flight-end', { type: 'geojson', data: place(last) });
		map.addLayer(module.dot('flight-end-dot', 'flight-end', 5, INK_ON_MAP, '#ffffff'));
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

	$effect(() => {
		if (!map || !setup) return;
		setup.shapes(map, 'flight-cursor')?.setData(cursor ? place(cursor) : NOTHING);
	});

	$effect(() => {
		if (!map || !setup || terrain3d === tilted) return;
		tilted = terrain3d;
		setup.setTerrain3d(map, terrain3d);
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
