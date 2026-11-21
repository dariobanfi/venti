<!--
	The map of the map tab: terrain, airspace, flying sites, live wind, the
	gliders of other pilots, every flight in the logbook as a line, and the
	pilot's own position when asked for.
-->
<script lang="ts">
	import { type AerialMaker, aerialMakers, photosCredit } from './aerial-credit';
	import { addLine } from './casing';
	import type { MapLayer } from './layers';
	import type { ImageSource } from 'maplibre-gl';
	import { onMount } from 'svelte';
	import { type AirspaceForMaps, type AirspaceOnMap, drawAirspace } from '#lib/airspace/on-map.ts';
	import { AIRSPACE_MIN_ZOOM } from '#lib/airspace/layers.ts';
	import type {
		Bounds,
		Coordinate,
		LiveGlider,
		SiteView,
		ThermalMapDay,
		ThermalMapKey,
		TrailPointView,
		WindStation
	} from '#lib/core/index.ts';
	import {
		ACCENT_ON_MAP,
		CLIMB_BANDS,
		INK_ON_MAP,
		LABEL_ON_MAP,
		PAPER_ON_MAP
	} from '#lib/design/palette.ts';
	import {
		arrowsLayer,
		dayFill,
		hourFill,
		layerAbove,
		THERMAL_ARROWS,
		THERMAL_SOURCE,
		thermalLayer
	} from '#lib/forecast/thermal-layer.ts';
	import { squareAt, squareFeatures } from '#lib/forecast/thermal-map.ts';
	import { RADAR, radarLayer, radarOpacity } from '#lib/radar/radar-layer.ts';
	import type { Corners } from '#lib/radar/radar.ts';
	import { gliderFeatures, LIVE_SOURCE, LIVE_TRACK_SOURCE, liveLayer } from '#lib/live/mark.ts';
	import { gliderPictures } from '#lib/live/symbols.ts';
	import { siteFeatures, SITES_SOURCE, sitesLayer } from '#lib/sites/mark.ts';
	import { sitePictures } from '#lib/sites/symbols.ts';
	import { stationFeatures } from '#lib/wind/mark.ts';
	import { windPictures } from '#lib/wind/symbols.ts';
	import { type Fix, type PositionProblem, watchPosition } from './position';
	import type { Map } from './setup';
	import { boundsAround, NOTHING, outlineLines, place, trackByBand } from './shapes';

	interface Props {
		/** The shapes of the logbook's flights. */
		outlines: Coordinate[][];
		/** The wind stations to draw. */
		stations: WindStation[];
		/** The flying sites to draw. */
		sites: SiteView[];
		/** The gliders of other pilots to draw. */
		gliders: LiveGlider[];
		/**
		 * The flight of the glider whose page is open, as far as it is known:
		 * drawn as a line in the colours of climb. Empty while none is.
		 */
		followed: readonly TrailPointView[];
		/**
		 * How far that glider's page, and what else lies over the map beside
		 * it, reaches in from each edge, in pixels.
		 */
		coveredByPage: { left: number; right: number; bottom: number };
		/** Where the airspace to draw comes from, once the app has it. */
		airspace: AirspaceForMaps | undefined;
		/** Whether the map keeps the pilot's position in its middle. */
		following: boolean;
		/** How far the navigation reaches in from the bottom edge, in pixels. */
		coveredBottom: number;
		/** Called when the position cannot be shown. */
		onproblem: (problem: PositionProblem) => void;
		/** Called with the part of the map in view, and its zoom, whenever the map has come to rest. */
		onregion: (region: Bounds, zoom: number) => void;
		/**
		 * Called whenever the map has come to rest with the short line of
		 * whose photographs of the aerial view can show there (`aerial-credit.ts`).
		 */
		onphotos?: (line: string) => void;
		/** Called with a station's identifier when the pilot chooses its arrow. */
		onstation: (id: string) => void;
		/** Called with a site's identifier when the pilot chooses its sign. */
		onsite: (id: string) => void;
		/** Called with a live track's identifier when the pilot chooses its glider. */
		onglider: (id: string) => void;
		/**
		 * Called with how many airspaces are drawn around what the map shows,
		 * or with nothing while the map is zoomed too far out to draw any.
		 */
		onairspace: (count: number | undefined) => void;
		/** The layers the pilot switched off with the map's chips. */
		hidden?: readonly MapLayer[];
		/** Whether the ground is photographs from above instead of the drawn map. */
		aerial?: boolean;
		/** The day of the thermal map whose squares to draw, while the "Forecast" layer is on. */
		thermalDay?: ThermalMapDay;
		/**
		 * One hour of a layer of the thermal map, drawn in place of the day:
		 * the squares, the arrows and the key their steps are coloured by.
		 */
		thermalHour?: {
			squares: GeoJSON.FeatureCollection;
			arrows: GeoJSON.FeatureCollection;
			legend: readonly ThermalMapKey[];
		};
		/**
		 * The rain radar's picture, while the "Radar" layer is on and has one:
		 * an address the map loads it from, the corners it is pinned by, and
		 * whether it is too old to be the rain of now and drawn fainter.
		 */
		radar?: { url: string; corners: Corners; faded: boolean };
		/** Called with a place clicked while an hour is drawn, where nothing else is under the pointer. */
		onhourat?: (at: { lat: number; lon: number }) => void;
		/**
		 * Called with a square's place in its day's list when the pilot
		 * chooses it where nothing else the map draws is under the pointer.
		 */
		onsquare?: (cell: number) => void;
		/**
		 * Called with the place the pilot clicked where nothing else the map
		 * draws is under the pointer, while the airspace layer is shown: for
		 * what airspace lies over it.
		 */
		onairspaceat?: (at: { lat: number; lon: number }) => void;
	}

	let {
		outlines,
		stations,
		sites,
		gliders,
		followed,
		coveredByPage,
		airspace,
		following = $bindable(),
		coveredBottom,
		onproblem,
		onregion,
		onphotos,
		onstation,
		onsite,
		onglider,
		onairspace,
		hidden = [],
		aerial = false,
		thermalDay,
		thermalHour,
		radar,
		onhourat,
		onsquare,
		onairspaceat
	}: Props = $props();

	/** The northern Alps between Lake Constance and Salzburg. */
	const START = { center: [11.6, 47.45] as [number, number], zoom: 7.4 };
	/** How close the map goes to a position it is asked to show. */
	const POSITION_ZOOM = 12.5;
	/**
	 * How far from an arrow, a sign or a glider a tap still chooses it, in
	 * pixels. A finger is wider than any of them.
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
	/** The aerial view's layers of photographs in the style the map loaded, read once. */
	let makers: AerialMaker[] | undefined;

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
	 * flights over it, then the flying sites, the wind above them, and the
	 * gliders of other pilots above everything.
	 */
	function addLayers(module: typeof import('./setup'), map: Map) {
		// The kind of day, square by square: over the ground and the roads,
		// under the base map's cable cars, borders and names and under
		// everything else this map draws.
		map.addSource(THERMAL_SOURCE, { type: 'geojson', data: NOTHING });
		map.addLayer(thermalLayer(), layerAbove(map.getStyle().layers ?? []));

		airspaceOnMap = drawAirspace(
			module,
			map,
			() => airspace,
			(count) => onairspace(count)
		);

		map.addSource('logbook-tracks', { type: 'geojson', data: NOTHING });
		// A white edge keeps the lines readable over any ground colour.
		addLine(map, module.line('logbook-tracks-edge', 'logbook-tracks', INK_ON_MAP, 5));
		map.addLayer(module.line('logbook-tracks-line', 'logbook-tracks', ACCENT_ON_MAP, 2.5));

		// The flight of a pilot who is being followed: a line in the colours
		// of climb, like a flight of the logbook when it is opened, over the
		// logbook's own lines.
		map.addSource(LIVE_TRACK_SOURCE, { type: 'geojson', data: NOTHING });
		addLine(map, module.line('live-track-edge', LIVE_TRACK_SOURCE, INK_ON_MAP, 5.5));
		CLIMB_BANDS.forEach((band, index) => {
			map.addLayer({
				...module.line(`live-track-${index}`, LIVE_TRACK_SOURCE, band.color, 3),
				filter: ['==', ['get', 'band'], index]
			});
		});

		// Flying sites are a layer of their own, with their own source and
		// their own credit: their licence asks that they stay apart.
		for (const picture of sitePictures()) {
			map.addImage(picture.name, picture.image, { pixelRatio: picture.pixelRatio });
		}
		map.addSource(SITES_SOURCE, { type: 'geojson', data: NOTHING });
		map.addLayer(sitesLayer(module.LABEL_FONT));

		for (const picture of windPictures()) {
			map.addImage(picture.name, picture.image, { pixelRatio: picture.pixelRatio });
		}
		// The forecast's arrows, hour by hour: under the stations, which stay on top.
		map.addSource(THERMAL_ARROWS, { type: 'geojson', data: NOTHING });
		map.addLayer(arrowsLayer(module.LABEL_FONT));
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
				'text-halo-color': PAPER_ON_MAP,
				'text-halo-width': 1.6
			}
		});
		// Other pilots in the air, above everything: a wing turned the way it
		// flies, with the pilot's name and altitude where there is room.
		for (const picture of gliderPictures()) {
			map.addImage(picture.name, picture.image, { pixelRatio: picture.pixelRatio });
		}
		map.addSource(LIVE_SOURCE, { type: 'geojson', data: NOTHING });
		map.addLayer(liveLayer(module.LABEL_FONT));

		map.on('click', (event) => choose(map, event.point));
		for (const layer of [LIVE_SOURCE, WIND, SITES_SOURCE, THERMAL_SOURCE]) {
			map.on('mouseenter', layer, () => (map.getCanvas().style.cursor = 'pointer'));
			map.on('mouseleave', layer, () => (map.getCanvas().style.cursor = ''));
		}

		map.addSource('position', { type: 'geojson', data: NOTHING });
		map.addLayer({
			id: 'position-halo',
			type: 'circle',
			source: 'position',
			paint: { 'circle-radius': 16, 'circle-color': ACCENT_ON_MAP, 'circle-opacity': 0.2 }
		});
		map.addLayer(module.dot('position-dot', 'position', 7, ACCENT_ON_MAP, PAPER_ON_MAP));
	}

	function tellRegion(map: Map) {
		const visible = map.getBounds();
		const region = {
			south: visible.getSouth(),
			west: visible.getWest(),
			north: visible.getNorth(),
			east: visible.getEast()
		};
		onregion(region, map.getZoom());
		makers ??= aerialMakers(map.getStyle());
		onphotos?.(photosCredit(makers, region, map.getZoom()));
	}

	/**
	 * A tap on a glider, a station's arrow or a site's sign, or close to it,
	 * opens the nearest. Gliders lie over the wind and the wind over the
	 * sites: what is on top wins where several are under the finger.
	 */
	function choose(map: Map, point: { x: number; y: number }) {
		for (const [layer, open] of [
			[LIVE_SOURCE, onglider],
			[WIND, onstation],
			[SITES_SOURCE, onsite]
		] as const) {
			const hits = map.queryRenderedFeatures(
				[
					[point.x - REACH, point.y - REACH],
					[point.x + REACH, point.y + REACH]
				],
				{ layers: [layer] }
			);
			const distance = (feature: (typeof hits)[number]) => {
				const at = map.project((feature.geometry as GeoJSON.Point).coordinates as [number, number]);
				return Math.hypot(at.x - point.x, at.y - point.y);
			};
			const nearest = hits.toSorted((first, second) => distance(first) - distance(second))[0];
			if (typeof nearest?.properties.id === 'string') {
				open(nearest.properties.id);
				return;
			}
		}
		// Nothing else under the pointer: the square of the thermal map there.
		const at = map.unproject([point.x, point.y]);
		if (thermalHour && onhourat) {
			onhourat({ lat: at.lat, lon: at.lng });
			return;
		}
		const square = thermalDay && squareAt(thermalDay, { lat: at.lat, lon: at.lng });
		if (square && thermalDay && onsquare) {
			onsquare(thermalDay.cells.indexOf(square));
			return;
		}
		// Else what airspace lies there, where the map draws airspace.
		if (!hidden.includes('airspace') && map.getZoom() >= AIRSPACE_MIN_ZOOM) {
			onairspaceat?.({ lat: at.lat, lon: at.lng });
		}
	}

	// Draws the airspace again when the app gets it, and when it changes:
	// a file was brought in, or the server's airspace was fetched.
	/** The map's own layers each chip switches, by the start of their names. */
	const LAYERS: Record<MapLayer, string[]> = {
		wind: [WIND],
		airspace: ['airspace-'],
		sites: [SITES_SOURCE],
		pilots: [LIVE_SOURCE, LIVE_TRACK_SOURCE, 'live-track']
	};

	$effect(() => {
		if (!map) return;
		const off = hidden.flatMap((layer) => LAYERS[layer]);
		for (const layer of map.getStyle().layers ?? []) {
			const mine = Object.values(LAYERS).some((starts) =>
				starts.some((start) => layer.id.startsWith(start))
			);
			if (!mine) continue;
			const hide = off.some((start) => layer.id.startsWith(start));
			map.setLayoutProperty(layer.id, 'visibility', hide ? 'none' : 'visible');
		}
	});

	$effect(() => {
		if (!map || !setup) return;
		setup.setAerial(map, aerial);
	});

	$effect(() => {
		if (!map || airspace?.revision === undefined) return;
		airspaceOnMap?.look();
	});

	// Draws the squares of the day looked at, or of the hour, and none while
	// the layer is off.
	$effect(() => {
		if (!map || !setup) return;
		if (thermalHour) {
			map.setPaintProperty(THERMAL_SOURCE, 'fill-color', hourFill(thermalHour.legend));
			setup.shapes(map, THERMAL_SOURCE)?.setData(thermalHour.squares);
			setup.shapes(map, THERMAL_ARROWS)?.setData(thermalHour.arrows);
		} else {
			map.setPaintProperty(THERMAL_SOURCE, 'fill-color', dayFill());
			setup.shapes(map, THERMAL_SOURCE)?.setData(squareFeatures(thermalDay));
			setup.shapes(map, THERMAL_ARROWS)?.setData(NOTHING);
		}
	});

	// Draws the radar's picture, another one when the frame changes, and
	// none while the layer is off. An image source cannot be without a
	// picture, so it comes and goes with the layer.
	$effect(() => {
		if (!map) return;
		const source = map.getSource<ImageSource>(RADAR);
		if (!radar) {
			if (map.getLayer(RADAR)) map.removeLayer(RADAR);
			if (source) map.removeSource(RADAR);
			return;
		}
		if (source) {
			source.updateImage({ url: radar.url, coordinates: radar.corners });
			map.setPaintProperty(RADAR, 'raster-opacity', radarOpacity(radar.faded));
		} else {
			map.addSource(RADAR, { type: 'image', url: radar.url, coordinates: radar.corners });
			// Over the forecast's squares, under what they lie under.
			map.addLayer(radarLayer(radar.faded), layerAbove(map.getStyle().layers ?? []));
		}
	});

	// Draws the flying sites when they change.
	$effect(() => {
		if (!map || !setup) return;
		setup.shapes(map, SITES_SOURCE)?.setData(siteFeatures(sites));
	});

	// Draws the stations when they change.
	$effect(() => {
		if (!map || !setup) return;
		setup.shapes(map, WIND)?.setData(stationFeatures(stations));
	});

	// Draws the gliders when they change.
	$effect(() => {
		if (!map || !setup) return;
		setup.shapes(map, LIVE_SOURCE)?.setData(gliderFeatures(gliders));
	});

	/** Whether the map has moved to show the flight that is being followed. */
	let followedInView = false;

	// Draws the followed flight when it grows, and when it first appears
	// moves the map once to show all of it beside the page that lies over
	// part of the map. When the page closes the line goes.
	$effect(() => {
		if (!map || !setup) return;
		setup.shapes(map, LIVE_TRACK_SOURCE)?.setData(trackByBand(followed));
		if (followed.length === 0) {
			followedInView = false;
			return;
		}
		if (followedInView) return;
		followedInView = true;
		const box = boundsAround([followed]);
		if (!box) return;
		// The map's own padding already keeps clear of the navigation.
		setup.showBeside(map, box, {
			left: coveredByPage.left,
			right: coveredByPage.right,
			bottom: Math.max(coveredByPage.bottom - coveredBottom, 0)
		});
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
