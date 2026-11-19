<!--
	The map of the map tab: terrain, airspace, live wind, every flight in the
	logbook as a line, and the pilot's own position when asked for.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { type AirspaceForMaps, type AirspaceOnMap, drawAirspace } from '#lib/airspace/on-map.ts';
	import type { Bounds, Coordinate, WindStation } from '#lib/core/index.ts';
	import { ACCENT_ON_MAP, LABEL_ON_MAP } from '#lib/design/palette.ts';
	import { stationFeatures } from '#lib/wind/mark.ts';
	import { windPictures } from '#lib/wind/symbols.ts';
	import { type Fix, type PositionProblem, watchPosition } from './position';
	import type { Map } from './setup';
	import { boundsAround, NOTHING, outlineLines, place } from './shapes';

	interface Props {
		/** The shapes of the logbook's flights. */
		outlines: Coordinate[][];
		/** The wind stations to draw. */
		stations: WindStation[];
		/** Where the airspace to draw comes from, once the app has it. */
		airspace: AirspaceForMaps | undefined;
		/** Whether the map keeps the pilot's position in its middle. */
		following: boolean;
		/** How far the navigation reaches in from the bottom edge, in pixels. */
		coveredBottom: number;
		/** Called when the position cannot be shown. */
		onproblem: (problem: PositionProblem) => void;
		/** Called with the part of the map in view whenever the map has come to rest. */
		onregion: (region: Bounds) => void;
		/** Called with a station's identifier when the pilot chooses its arrow. */
		onstation: (id: string) => void;
		/**
		 * Called with how many airspaces are drawn around what the map shows,
		 * or with nothing while the map is zoomed too far out to draw any.
		 */
		onairspace: (count: number | undefined) => void;
	}

	let {
		outlines,
		stations,
		airspace,
		following = $bindable(),
		coveredBottom,
		onproblem,
		onregion,
		onstation,
		onairspace
	}: Props = $props();

	/** The northern Alps between Lake Constance and Salzburg. */
	const START = { center: [11.6, 47.45] as [number, number], zoom: 7.4 };
	/** How close the map goes to a position it is asked to show. */
	const POSITION_ZOOM = 12.5;
	/**
	 * How far from an arrow a tap still chooses its station, in pixels. A
	 * finger is wider than an arrow.
	 */
	const REACH = 22;

	const WIND = 'wind-stations';

	let container: HTMLDivElement;
	let setup: typeof import('./setup') | undefined;
	/** The map, once its style is there to draw on. */
	let map = $state.raw<Map>();
	let lastFix: Fix | undefined;
	let stopWatching: (() => void) | undefined;
	let airspaceOnMap: AirspaceOnMap | undefined;

	onMount(() => {
		let gone = false;
		let created: Map | undefined;
		void import('./setup').then((module) => {
			if (gone) return;
			setup = module;
			created = module.createMap(container, 'Map', START);
			// Dragging the map away means the pilot wants to look elsewhere.
			created.on('dragstart', () => (following = false));
			const styled = created;
			module.whenStyled(styled, () => {
				addLayers(module, styled);
				map = styled;
				tellRegion(styled);
				styled.on('moveend', () => tellRegion(styled));
			});
		});
		return () => {
			gone = true;
			stopWatching?.();
			created?.remove();
		};
	});

	/**
	 * What this map draws on top of the base map: airspace lowest, the
	 * flights over it, the wind above them.
	 */
	function addLayers(module: typeof import('./setup'), map: Map) {
		airspaceOnMap = drawAirspace(
			module,
			map,
			() => airspace,
			(count) => onairspace(count)
		);

		map.addSource('logbook-tracks', { type: 'geojson', data: NOTHING });
		// A white edge keeps the lines readable over any ground colour.
		map.addLayer(module.line('logbook-tracks-edge', 'logbook-tracks', '#ffffff', 5));
		map.addLayer(module.line('logbook-tracks-line', 'logbook-tracks', ACCENT_ON_MAP, 2.5));

		for (const picture of windPictures()) {
			map.addImage(picture.name, picture.image, { pixelRatio: picture.pixelRatio });
		}
		map.addSource(WIND, { type: 'geojson', data: NOTHING });
		map.addLayer({
			id: WIND,
			type: 'symbol',
			source: WIND,
			layout: {
				'icon-image': ['get', 'picture'],
				'icon-rotate': ['get', 'heading'],
				// Turned against north, not against the screen: the arrow keeps
				// pointing where the wind blows if the map is ever rotated.
				'icon-rotation-alignment': 'map',
				'icon-size': ['get', 'scale'],
				'text-field': ['get', 'label'],
				'text-font': [module.LABEL_FONT],
				'text-size': 12,
				'text-anchor': 'top',
				'text-offset': [0, 1.15],
				// Where stations crowd, the windiest are placed first and the
				// others give way; a station keeps its arrow when only its
				// numbers have no room.
				'symbol-sort-key': ['get', 'rank'],
				'text-optional': true
			},
			paint: {
				'text-color': LABEL_ON_MAP,
				'text-halo-color': '#ffffff',
				'text-halo-width': 1.6
			}
		});
		map.on('click', (event) => choose(map, event.point));
		map.on('mouseenter', WIND, () => (map.getCanvas().style.cursor = 'pointer'));
		map.on('mouseleave', WIND, () => (map.getCanvas().style.cursor = ''));

		map.addSource('position', { type: 'geojson', data: NOTHING });
		map.addLayer({
			id: 'position-halo',
			type: 'circle',
			source: 'position',
			paint: { 'circle-radius': 16, 'circle-color': ACCENT_ON_MAP, 'circle-opacity': 0.2 }
		});
		map.addLayer(module.dot('position-dot', 'position', 7, ACCENT_ON_MAP, '#ffffff'));
	}

	function tellRegion(map: Map) {
		const visible = map.getBounds();
		onregion({
			south: visible.getSouth(),
			west: visible.getWest(),
			north: visible.getNorth(),
			east: visible.getEast()
		});
	}

	/** A tap on a station's arrow, or close to it, opens the nearest station. */
	function choose(map: Map, point: { x: number; y: number }) {
		const hits = map.queryRenderedFeatures(
			[
				[point.x - REACH, point.y - REACH],
				[point.x + REACH, point.y + REACH]
			],
			{ layers: [WIND] }
		);
		const distance = (feature: (typeof hits)[number]) => {
			const at = map.project((feature.geometry as GeoJSON.Point).coordinates as [number, number]);
			return Math.hypot(at.x - point.x, at.y - point.y);
		};
		const nearest = hits.toSorted((first, second) => distance(first) - distance(second))[0];
		if (typeof nearest?.properties.id === 'string') onstation(nearest.properties.id);
	}

	// Draws the airspace again when the app gets it, and when it changes:
	// a file was brought in, or the server's airspace was fetched.
	$effect(() => {
		if (!map || airspace?.revision === undefined) return;
		airspaceOnMap?.look();
	});

	// Draws the stations when they change.
	$effect(() => {
		if (!map || !setup) return;
		setup.shapes(map, WIND)?.setData(stationFeatures(stations));
	});

	// Draws the flights when they change, and moves the camera to show them
	// all. A map that is following the pilot stays where it is.
	$effect(() => {
		if (!map || !setup) return;
		setup.shapes(map, 'logbook-tracks')?.setData(outlineLines(outlines));
		const box = boundsAround(outlines);
		const covered = { top: 0, right: 0, bottom: coveredBottom, left: 0 };
		if (box && !lastFix) setup.showBounds(map, box, covered);
		else map.setPadding(covered);
	});

	// Starts and stops following. The position stays on the map once found;
	// following only decides whether the map moves with it.
	$effect(() => {
		if (!map || !following) return;
		if (lastFix) centre(map, lastFix);
		stopWatching ??= watchPosition(
			navigator.geolocation,
			(fix) => {
				lastFix = fix;
				if (!map) return;
				setup?.shapes(map, 'position')?.setData(place(fix));
				if (following) centre(map, fix);
			},
			(problem) => {
				stopWatching = undefined;
				following = false;
				onproblem(problem);
			}
		);
	});

	function centre(map: Map, fix: Fix) {
		map.easeTo({ center: [fix.lon, fix.lat], zoom: Math.max(map.getZoom(), POSITION_ZOOM) });
	}

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
