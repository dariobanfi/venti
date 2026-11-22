<!--
	The map of the map tab: terrain, airspace, flying sites, live wind, the
	gliders of other pilots, cables, every flight in the logbook as a line,
	and the pilot's own position when asked for.
-->
<script lang="ts">
	import { type AerialMaker, aerialMakers, photosCredit } from './aerial-credit';
	import { addLine, CASING } from './casing';
	import type { MapLayer } from './layers';
	import type { ExpressionSpecification, ImageSource } from 'maplibre-gl';
	import { onMount, untrack } from 'svelte';
	import {
		CLIMBS_LABELS,
		CLIMBS_SOURCE,
		climbFeatures,
		climbLayers,
		FLOWN_SOURCE,
		flownFeatures,
		flownLayer
	} from '#lib/air/on-map.ts';
	import type { ClimbView, FlownCellView } from '#lib/core/index.ts';
	import { type AirspaceForMaps, type AirspaceOnMap, drawAirspace } from '#lib/airspace/on-map.ts';
	import { AIRSPACE_MIN_ZOOM } from '#lib/airspace/layers.ts';
	import {
		CABLES_MAPPED,
		CABLES_SOURCE,
		CABLES_UNMAPPED,
		cableFeatures,
		pixelsFromLine
	} from '#lib/cables/on-map.ts';
	import type {
		Bounds,
		CableLine,
		Coordinate,
		GlideReach,
		LiveGlider,
		LiveOthersNetwork,
		RoutePlan,
		SiteCandidateView,
		SiteView,
		ThermalMapDay,
		ThermalMapKey,
		TrailPointView,
		WindStation
	} from '#lib/core/index.ts';
	import {
		ACCENT_ON_MAP,
		CLIMB_BANDS,
		DANGER_ON_MAP,
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
	import {
		isRadar,
		radarBefore,
		radarId,
		radarLayer,
		radarOpacity
	} from '#lib/radar/radar-layer.ts';
	import type { OnMap } from '#lib/radar/radar.svelte.ts';
	import { SLOPES_OPACITY } from '#lib/slopes/grid.ts';
	import type { SlopesPicture } from '#lib/slopes/model.svelte.ts';
	import {
		closingOutline,
		drawnLine,
		faiOutlines,
		numberedPlaces,
		ROUTE_CLOSING,
		ROUTE_DOTS,
		ROUTE_DRAWN,
		ROUTE_FAI,
		ROUTE_PLACES,
		ROUTE_SCORED,
		scoredLine
	} from '#lib/route/on-map.ts';
	import {
		REACH_AREA,
		REACH_EDGE,
		REACH_FILL,
		REACH_START,
		reachArea,
		reachEdge,
		reachStart
	} from '#lib/reach/on-map.ts';
	import { gliderFeatures, LIVE_SOURCE, LIVE_TRACK_SOURCE, liveLayer } from '#lib/live/mark.ts';
	import { OTHERS_LAYERS, OTHERS_SOURCE, otherFeatures, othersLayers } from '#lib/live/others.ts';
	import { gliderPictures } from '#lib/live/symbols.ts';
	import {
		candidateFeatures,
		CANDIDATES_SOURCE,
		candidatesLayer,
		SITE_OUTLINES_SOURCE,
		siteFeatures,
		siteOutlineLayers,
		siteOutlines,
		SITES_SOURCE,
		sitesLayer
	} from '#lib/sites/mark.ts';
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
		/** The flying sites to draw: those the pilot's filter leaves. */
		sites: readonly SiteView[];
		/**
		 * The take-offs whose source does not say what the filter asks: drawn
		 * apart from the others, as muted signs.
		 */
		sitesApart?: readonly SiteView[];
		/**
		 * The places where pilots take off and no site stands, while their
		 * layer is on: drawn as small rings under the sites' signs.
		 */
		candidates?: readonly SiteCandidateView[];
		/** Called with a candidate's key (`candidateKey`) when the pilot chooses its ring. */
		oncandidate?: (key: string) => void;
		/** The gliders of other pilots to draw. */
		gliders: LiveGlider[];
		/**
		 * The aircraft other networks hear (the Open Glider Network), drawn
		 * under Venti's own pilots in a muted colour.
		 */
		others?: readonly LiveOthersNetwork[];
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
		/**
		 * The cables to draw, while the map is close enough: power lines and
		 * ropeways, solid where their height is mapped, dashed where not.
		 */
		cables?: readonly CableLine[];
		/** The climbs of the last minutes, while their layer is on. */
		climbs?: readonly ClimbView[];
		/** Called with the key (`airKey`) of the climb the pilot chose. */
		onclimb?: (key: string) => void;
		/** The cells in which thermals have been flown, while their layer is on. */
		flown?: readonly FlownCellView[];
		/** Called with the key (`airKey`) of the cell the pilot chose. */
		onflown?: (key: string) => void;
		/** Called when the map's own credits are unfolded or folded with their button. */
		oncredits?: (open: boolean) => void;
		/** Called with a cable's identifier when the pilot clicks on or close to its line. */
		oncable?: (id: string) => void;
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
		/** Called with an aircraft's identifier when the pilot chooses one another network hears. */
		onother?: (id: string) => void;
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
		 * The rain radar's pictures, while the "Radar" layer is on and has
		 * some, the lowest first: for each of the radar's layers that has one
		 * an address the map loads it from, the corners it is pinned by, and
		 * whether that layer is too old to be the rain of now and drawn fainter.
		 */
		radar?: readonly OnMap[];
		/**
		 * How steep the ground is, while the "Slopes" layer is on and the map
		 * close enough: one picture of the part of the map asked about.
		 */
		slopes?: SlopesPicture;
		/** Called with a place clicked while an hour is drawn, where nothing else is under the pointer. */
		onhourat?: (at: { lat: number; lon: number }) => void;
		/**
		 * Called with a square's place in its day's list when the pilot
		 * chooses it where nothing else the map draws is under the pointer.
		 */
		onsquare?: (cell: number) => void;
		/**
		 * Called with the place the pilot clicked where nothing else the map
		 * draws is under the pointer: for what lies there and what can be
		 * done from there. `airspace` says whether the airspace layer is
		 * shown, so that what airspace lies over it is said.
		 */
		onplaceat?: (at: { lat: number; lon: number }, airspace: boolean) => void;
		/**
		 * What a glide reaches from a place, while it is shown: the place and
		 * the core's answer, once it has one.
		 */
		reach?: { from: Coordinate; reach: GlideReach | undefined };
		/**
		 * Whether the pilot is choosing a place for a site: a mark stands
		 * over the middle of the map, which is moved under it, and a click
		 * chooses nothing. `middle` says where the mark is.
		 */
		placing?: boolean;
		/**
		 * The route being planned, while the planner is open: its places in
		 * order and what the core says of it. A click then adds a place, and
		 * a place is dragged to move it.
		 */
		route?: {
			places: readonly Coordinate[];
			plan: RoutePlan | undefined;
			/** The height to leave each place at, where the core says one, metres. */
			departures?: readonly (number | undefined)[];
		};
		/**
		 * Called with the place clicked while a route is planned: a site's
		 * sign gives the site's place and its identifier, a place of the
		 * route its own place again.
		 */
		onrouteat?: (at: Coordinate, siteId?: string) => void;
		/** Called when a place of the route was dragged elsewhere, with its place in the route. */
		onroutemove?: (index: number, to: Coordinate) => void;
		/** Where the map opens, close enough to see the valleys; the northern Alps without one. Read once. */
		around?: Coordinate;
	}

	let {
		outlines,
		stations,
		sites,
		sitesApart = [],
		candidates = [],
		oncandidate,
		gliders,
		others = [],
		followed,
		coveredByPage,
		cables = [],
		oncable,
		climbs = [],
		onclimb,
		flown = [],
		onflown,
		oncredits,
		airspace,
		following = $bindable(),
		coveredBottom,
		onproblem,
		onregion,
		onphotos,
		onstation,
		onsite,
		onglider,
		onother,
		onairspace,
		hidden = [],
		aerial = false,
		thermalDay,
		thermalHour,
		radar,
		slopes,
		onhourat,
		onsquare,
		onplaceat,
		reach,
		placing = false,
		route,
		onrouteat,
		onroutemove,
		around
	}: Props = $props();

	/** The northern Alps between Lake Constance and Salzburg. */
	const START = { center: [11.6, 47.45] as [number, number], zoom: 7.4 };
	/** How close the map goes to a place it is asked to show around. */
	const AROUND_ZOOM = 10.5;
	/** How close the map goes to a site's place that is to be moved. */
	const PLACING_ZOOM = 15;
	/** How close the map goes to a position it is asked to show. */
	const POSITION_ZOOM = 12.5;
	/**
	 * How far from an arrow, a sign or a glider a tap still chooses it, in
	 * pixels. A finger is wider than any of them.
	 */
	const REACH = 22;

	const WIND = 'wind-stations';
	/** The map's source and layer of the slopes' picture. */
	const SLOPES = 'slopes';

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
			const at = untrack(() => around);
			created = module.createMap(
				container,
				'Map',
				at ? { center: [at.lon, at.lat], zoom: AROUND_ZOOM } : START,
				// The credits stay folded behind their button at every width.
				(open) => oncredits?.(open)
			);
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

		addCables(module, map);

		map.addSource('logbook-tracks', { type: 'geojson', data: NOTHING });
		// A white edge keeps the lines readable over any ground colour.
		addLine(map, module.line('logbook-tracks-edge', 'logbook-tracks', INK_ON_MAP, 5));
		map.addLayer(module.line('logbook-tracks-line', 'logbook-tracks', ACCENT_ON_MAP, 2.5));

		addReach(module, map);

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
		// A site mapped as an area has its outline under its sign.
		map.addSource(SITE_OUTLINES_SOURCE, { type: 'geojson', data: NOTHING });
		for (const layer of siteOutlineLayers()) map.addLayer(layer);
		// The take-offs without a site, rings under the signs.
		map.addSource(CANDIDATES_SOURCE, { type: 'geojson', data: NOTHING });
		map.addLayer(candidatesLayer());
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
		// What many gliders say together, under the gliders themselves:
		// where thermals have been, and over it the climbs of now.
		map.addSource(FLOWN_SOURCE, { type: 'geojson', data: NOTHING });
		map.addLayer(flownLayer());
		map.addSource(CLIMBS_SOURCE, { type: 'geojson', data: NOTHING });
		for (const layer of climbLayers(module.LABEL_FONT)) map.addLayer(layer);

		// Other pilots in the air, above everything: a wing turned the way it
		// flies, with the pilot's name and altitude where there is room.
		for (const picture of gliderPictures()) {
			map.addImage(picture.name, picture.image, { pixelRatio: picture.pixelRatio });
		}
		// The aircraft other networks hear, under Venti's own pilots.
		map.addSource(OTHERS_SOURCE, { type: 'geojson', data: NOTHING });
		for (const layer of othersLayers(module.LABEL_FONT)) map.addLayer(layer);
		map.addSource(LIVE_SOURCE, { type: 'geojson', data: NOTHING });
		map.addLayer(liveLayer(module.LABEL_FONT));

		addRoute(module, map);

		map.on('click', (event) => choose(map, event.point));
		for (const layer of [
			LIVE_SOURCE,
			...OTHERS_LAYERS,
			WIND,
			SITES_SOURCE,
			CANDIDATES_SOURCE,
			CABLES_MAPPED,
			CABLES_UNMAPPED,
			CLIMBS_SOURCE,
			FLOWN_SOURCE,
			THERMAL_SOURCE
		]) {
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

	/**
	 * The cables, over the airspace and under the flights: a line in the
	 * pack's danger colour over the light casing that the aerial view
	 * shows, solid where the cable's height is mapped and dashed where it
	 * is not, as the phones draw them.
	 */
	function addCables(module: typeof import('./setup'), map: Map) {
		map.addSource(CABLES_SOURCE, { type: 'geojson', data: NOTHING });
		const mapped = module.line(CABLES_MAPPED, CABLES_SOURCE, DANGER_ON_MAP, 2);
		addLine(map, { ...mapped, filter: ['!', ['get', 'dashed']] });
		const unmapped = module.line(CABLES_UNMAPPED, CABLES_SOURCE, DANGER_ON_MAP, 2);
		addLine(map, {
			...unmapped,
			layout: { 'line-join': 'round' },
			filter: ['get', 'dashed'],
			paint: { ...unmapped.paint, 'line-dasharray': [2, 1.5] }
		});
	}

	/**
	 * What a glide reaches, over the tracks and under the sites: the area
	 * in the pack's primary at an airspace's low fill, its edge in the same
	 * colour over the tracks' dark casing (and the light one over
	 * photographs), dashed and without the casing where the ground loaded
	 * ends; the place it starts from, the small dot a flight starts at.
	 */
	function addReach(module: typeof import('./setup'), map: Map) {
		map.addSource(REACH_AREA, { type: 'geojson', data: NOTHING });
		map.addLayer({
			id: 'reach-area-fill',
			type: 'fill',
			source: REACH_AREA,
			paint: { 'fill-color': ACCENT_ON_MAP, 'fill-opacity': REACH_FILL }
		});
		map.addSource(REACH_EDGE, { type: 'geojson', data: NOTHING });
		const solid: ExpressionSpecification = ['!', ['get', 'dashed']];
		addLine(map, { ...module.line('reach-edge-casing', REACH_EDGE, INK_ON_MAP, 5), filter: solid });
		map.addLayer({
			...module.line('reach-edge-line', REACH_EDGE, ACCENT_ON_MAP, 2),
			filter: solid
		});
		const dashed = module.line('reach-edge-unknown', REACH_EDGE, ACCENT_ON_MAP, 2);
		map.addLayer({
			...dashed,
			layout: { 'line-join': 'round' },
			filter: ['get', 'dashed'],
			paint: { ...dashed.paint, 'line-dasharray': [2, 2] }
		});
		map.addSource(REACH_START, { type: 'geojson', data: NOTHING });
		map.addLayer(module.dot('reach-start-dot', REACH_START, 5, PAPER_ON_MAP, INK_ON_MAP));
	}

	/**
	 * The route being planned, over everything else: where a third place
	 * makes an FAI triangle of the first leg, dashed; the line through the
	 * places, with the light casing over photographs; the route as it would
	 * be scored, dashed in the app's colour; the places with their numbers.
	 * As the phones draw it.
	 */
	function addRoute(module: typeof import('./setup'), map: Map) {
		map.addSource(ROUTE_FAI, { type: 'geojson', data: NOTHING });
		// Where a triangle closes: a thin dashed ring in the map's ink.
		map.addSource(ROUTE_CLOSING, { type: 'geojson', data: NOTHING });
		const closing = module.line('route-closing-line', ROUTE_CLOSING, INK_ON_MAP, 1.5);
		addLine(map, {
			...closing,
			layout: { 'line-join': 'round' },
			paint: { ...closing.paint, 'line-dasharray': [2, 2] }
		});
		const areas = module.line('route-fai-line', ROUTE_FAI, ACCENT_ON_MAP, 1.5);
		map.addLayer({
			...areas,
			paint: { ...areas.paint, 'line-dasharray': [3, 2], 'line-opacity': 0.85 }
		});
		map.addSource(ROUTE_DRAWN, { type: 'geojson', data: NOTHING });
		addLine(map, module.line('route-drawn-line', ROUTE_DRAWN, INK_ON_MAP, 3));
		map.addSource(ROUTE_SCORED, { type: 'geojson', data: NOTHING });
		const scored = module.line('route-scored-line', ROUTE_SCORED, ACCENT_ON_MAP, 2);
		addLine(map, { ...scored, paint: { ...scored.paint, 'line-dasharray': [3, 2] } });
		map.addSource(ROUTE_PLACES, { type: 'geojson', data: NOTHING });
		map.addLayer(module.dot(ROUTE_DOTS, ROUTE_PLACES, 11, PAPER_ON_MAP, INK_ON_MAP));
		map.addLayer({
			id: 'route-places-number',
			type: 'symbol',
			source: ROUTE_PLACES,
			layout: {
				'text-field': ['get', 'label'],
				'text-font': [module.LABEL_FONT],
				'text-size': 13,
				'text-allow-overlap': true,
				'text-ignore-placement': true
			},
			paint: { 'text-color': INK_ON_MAP }
		});
		// Under each place the height to leave it at, where the core says one.
		map.addLayer({
			id: 'route-places-height',
			type: 'symbol',
			source: ROUTE_PLACES,
			layout: {
				'text-field': ['get', 'height'],
				'text-font': [module.LABEL_FONT],
				'text-size': 12,
				'text-anchor': 'top',
				'text-offset': [0, 1.1],
				'text-optional': true
			},
			paint: {
				'text-color': LABEL_ON_MAP,
				'text-halo-color': PAPER_ON_MAP,
				'text-halo-width': 1.6
			}
		});
		map.on('mouseenter', ROUTE_DOTS, () => (map.getCanvas().style.cursor = 'grab'));
		map.on('mouseleave', ROUTE_DOTS, () => (map.getCanvas().style.cursor = ''));

		// A place of the route is dragged to move it: the map does not pan
		// meanwhile, and the line follows the pointer until it is let go.
		const startDrag = (
			event: { preventDefault: () => void; features?: { properties: Record<string, unknown> }[] },
			moving: 'mousemove' | 'touchmove',
			ending: 'mouseup' | 'touchend'
		) => {
			const index = event.features?.[0]?.properties.index;
			if (!route || typeof index !== 'number') return;
			event.preventDefault();
			const from = route.places[index];
			if (!from) return;
			let to: Coordinate | undefined;
			map.getCanvas().style.cursor = 'grabbing';
			const moved = (event: { lngLat: { lat: number; lng: number } }) => {
				to = { lat: event.lngLat.lat, lon: event.lngLat.lng };
				const places = (route?.places ?? []).map((place) =>
					place.lat === from.lat && place.lon === from.lon ? (to ?? place) : place
				);
				setup?.shapes(map, ROUTE_DRAWN)?.setData(drawnLine(places));
				setup?.shapes(map, ROUTE_PLACES)?.setData(numberedPlaces(places));
			};
			map.on(moving, moved);
			map.once(ending, () => {
				map.off(moving, moved);
				map.getCanvas().style.cursor = '';
				if (to) onroutemove?.(index, to);
			});
		};
		map.on('mousedown', ROUTE_DOTS, (event) => startDrag(event, 'mousemove', 'mouseup'));
		map.on('touchstart', ROUTE_DOTS, (event) => {
			if (event.points.length === 1) startDrag(event, 'touchmove', 'touchend');
		});
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
		// A map says nothing of where it has come to rest; the browser tests read it here.
		(globalThis as { ventiView?: unknown }).ventiView = { zoom: map.getZoom(), ...region };
		makers ??= aerialMakers(map.getStyle());
		onphotos?.(photosCredit(makers, region, map.getZoom()));
	}

	/**
	 * A tap on a glider, a station's arrow or a site's sign, or close to it,
	 * opens the nearest. Gliders lie over the wind and the wind over the
	 * sites: what is on top wins where several are under the finger.
	 */
	function choose(map: Map, point: { x: number; y: number }) {
		// While a place is chosen the map is only moved.
		if (placing) return;
		if (route) {
			planAt(map, point);
			return;
		}
		for (const [layers, open, chip] of [
			[[LIVE_SOURCE], onglider, 'pilots'],
			[OTHERS_LAYERS, (id: string) => onother?.(id), 'pilots'],
			[[WIND], onstation, 'wind'],
			[[SITES_SOURCE], onsite, 'sites'],
			[[CANDIDATES_SOURCE], (key: string) => oncandidate?.(key), 'sites']
		] as const) {
			// A layer just switched off can still be in the frame drawn last.
			if (hidden.includes(chip)) continue;
			const hits = map.queryRenderedFeatures(
				[
					[point.x - REACH, point.y - REACH],
					[point.x + REACH, point.y + REACH]
				],
				{ layers: [...layers] }
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
		// A climb of now, then a cell where thermals have been.
		for (const [layers, open] of [
			[[CLIMBS_SOURCE, CLIMBS_LABELS], onclimb],
			[[FLOWN_SOURCE], onflown]
		] as const) {
			const hit = map.queryRenderedFeatures(
				[
					[point.x - REACH, point.y - REACH],
					[point.x + REACH, point.y + REACH]
				],
				{ layers: [...layers] }
			)[0];
			if (open && typeof hit?.properties.id === 'string') {
				open(hit.properties.id);
				return;
			}
		}
		// A cable on or near the pointer: the nearest line.
		const cable = hidden.includes('cables') ? undefined : cableAt(map, point);
		if (cable) {
			oncable?.(cable);
			return;
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
		// Else the place itself: what airspace lies there, where the map
		// draws airspace, and what can be done from there.
		onplaceat?.(
			{ lat: at.lat, lon: at.lng },
			!hidden.includes('airspace') && map.getZoom() >= AIRSPACE_MIN_ZOOM
		);
	}

	/** The cable whose line passes nearest the pointer, within a finger's reach. */
	function cableAt(map: Map, point: { x: number; y: number }): string | undefined {
		const hits = map.queryRenderedFeatures(
			[
				[point.x - REACH, point.y - REACH],
				[point.x + REACH, point.y + REACH]
			],
			{ layers: [CABLES_MAPPED, CABLES_UNMAPPED] }
		);
		let nearest: { id: string; pixels: number } | undefined;
		for (const hit of hits) {
			const id = hit.properties.id;
			if (typeof id !== 'string' || hit.geometry.type !== 'LineString') continue;
			const line = hit.geometry.coordinates.map((at) => map.project(at as [number, number]));
			const pixels = pixelsFromLine(point, line);
			if (pixels <= REACH && (!nearest || pixels < nearest.pixels)) nearest = { id, pixels };
		}
		return nearest?.id;
	}

	/**
	 * A click while a route is planned: on a place of the route, that place
	 * again, as to close a triangle; on a site's sign, the site; else the
	 * place under the pointer.
	 */
	function planAt(map: Map, point: { x: number; y: number }) {
		const near = (layer: string) => {
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
			return hits.toSorted((first, second) => distance(first) - distance(second))[0];
		};
		const own = near(ROUTE_DOTS)?.properties.index;
		const again = typeof own === 'number' ? route?.places[own] : undefined;
		if (again) {
			onrouteat?.({ lat: again.lat, lon: again.lon });
			return;
		}
		const site = hidden.includes('sites') ? undefined : near(SITES_SOURCE);
		if (site && typeof site.properties.id === 'string') {
			const [lon, lat] = (site.geometry as GeoJSON.Point).coordinates;
			onrouteat?.({ lat, lon }, site.properties.id);
			return;
		}
		const at = map.unproject([point.x, point.y]);
		onrouteat?.({ lat: at.lat, lon: at.lng });
	}

	// Draws what a glide reaches when it changes, and nothing once cleared.
	$effect(() => {
		if (!map || !setup) return;
		setup.shapes(map, REACH_AREA)?.setData(reachArea(reach?.reach));
		const edge = reachEdge(reach?.reach);
		setup.shapes(map, REACH_EDGE)?.setData(edge);
		setup.shapes(map, REACH_START)?.setData(reachStart(reach?.from));
		// A map says nothing of what it draws; the browser tests read it here.
		(globalThis as { ventiReach?: unknown }).ventiReach = {
			outline: reach?.reach?.outline.length ?? 0,
			dashed: edge.features.filter((feature) => feature.properties?.dashed).length
		};
	});

	// Draws the route being planned when it changes, and none while the
	// planner is closed. A double click adds two places, not a zoom.
	$effect(() => {
		if (!map || !setup) return;
		const places = route?.places ?? [];
		setup.shapes(map, ROUTE_DRAWN)?.setData(drawnLine(places));
		setup.shapes(map, ROUTE_PLACES)?.setData(numberedPlaces(places, route?.departures));
		setup.shapes(map, ROUTE_SCORED)?.setData(scoredLine(route?.plan));
		setup.shapes(map, ROUTE_FAI)?.setData(faiOutlines(route?.plan));
		setup.shapes(map, ROUTE_CLOSING)?.setData(closingOutline(route?.plan));
		if (route) map.doubleClickZoom.disable();
		else map.doubleClickZoom.enable();
		// A map says nothing of what it draws; the browser tests read it here.
		(globalThis as { ventiRoute?: unknown }).ventiRoute = {
			places: places.length,
			scored: route?.plan?.score?.route.length ?? 0,
			closing: route?.plan?.closingCircle?.outline.length ?? 0
		};
	});

	// Draws the airspace again when the app gets it, and when it changes:
	// a file was brought in, or the server's airspace was fetched.
	/** The map's own layers each chip switches, by the start of their names. */
	const LAYERS: Record<MapLayer, string[]> = {
		wind: [WIND],
		airspace: ['airspace-'],
		cables: [CABLES_SOURCE],
		sites: [SITES_SOURCE],
		pilots: [LIVE_SOURCE, LIVE_TRACK_SOURCE, 'live-track', OTHERS_SOURCE]
	};

	$effect(() => {
		if (!map) return;
		const off = hidden.flatMap((layer) => LAYERS[layer]);
		for (const layer of map.getStyle().layers ?? []) {
			const mine = Object.values(LAYERS).some((starts) =>
				starts.some((start) => layer.id.startsWith(start))
			);
			if (!mine) continue;
			// A casing shows only over the photographs (`setAerial`).
			const hide =
				off.some((start) => layer.id.startsWith(start)) || (layer.id.endsWith(CASING) && !aerial);
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

	// Draws the radar's pictures, each of the radar's layers as a source and
	// a layer of its own, others when the moment changes, and none while
	// the radar is off. An image source cannot be without a picture, so it
	// comes and goes with its layer's picture.
	$effect(() => {
		if (!map) return;
		const wanted = radar ?? [];
		for (const id of map.getLayersOrder().filter(isRadar)) {
			if (wanted.some((picture) => radarId(picture.layer) === id)) continue;
			map.removeLayer(id);
			map.removeSource(id);
		}
		for (const picture of wanted) {
			const id = radarId(picture.layer);
			const source = map.getSource<ImageSource>(id);
			if (source) {
				// A picture that stays is not loaded again.
				if (
					source.url !== picture.url ||
					source.coordinates.flat().join() !== picture.corners.flat().join()
				) {
					source.updateImage({ url: picture.url, coordinates: picture.corners });
				}
				map.setPaintProperty(id, 'raster-opacity', radarOpacity(picture.faded));
				continue;
			}
			map.addSource(id, { type: 'image', url: picture.url, coordinates: picture.corners });
			// Over the forecast's squares, under what they lie under, and
			// under the radar's own layers that come above this one.
			map.addLayer(
				radarLayer(picture.layer, picture.faded),
				radarBefore(picture.layer, map.getLayersOrder(), layerAbove(map.getStyle().layers ?? []))
			);
		}
		// A map says nothing of what it draws; the browser tests read it here.
		(globalThis as { ventiRadar?: unknown }).ventiRadar = map.getLayersOrder().filter(isRadar);
	});

	// Draws the slopes as one picture pinned by its corners, a cell a
	// pixel and without smoothing, under the forecast's squares and the
	// radar; none while the layer is off. An image source cannot be without
	// a picture, so it comes and goes with the picture.
	$effect(() => {
		if (!map) return;
		const source = map.getSource<ImageSource>(SLOPES);
		if (!slopes) {
			if (source) {
				map.removeLayer(SLOPES);
				map.removeSource(SLOPES);
			}
		} else if (source) {
			if (source.url !== slopes.url) {
				source.updateImage({ url: slopes.url, coordinates: slopes.corners });
			}
		} else {
			map.addSource(SLOPES, { type: 'image', url: slopes.url, coordinates: slopes.corners });
			map.addLayer(
				{
					id: SLOPES,
					type: 'raster',
					source: SLOPES,
					paint: {
						'raster-resampling': 'nearest',
						'raster-opacity': SLOPES_OPACITY,
						'raster-fade-duration': 0
					}
				},
				THERMAL_SOURCE
			);
		}
		// A map says nothing of what it draws; the browser tests read it here.
		(globalThis as { ventiSlopes?: unknown }).ventiSlopes = {
			drawn: Boolean(map.getLayer(SLOPES)),
			cells: slopes?.cells ?? 0,
			classed: slopes?.classed ?? 0,
			byClass: slopes?.byClass ?? []
		};
	});

	// Draws the flying sites when they change: the signs, and under them the
	// outlines of those mapped as areas.
	$effect(() => {
		if (!map || !setup) return;
		const signs = siteFeatures(sites, sitesApart);
		const areas = siteOutlines(sites, sitesApart);
		setup.shapes(map, SITES_SOURCE)?.setData(signs);
		setup.shapes(map, SITE_OUTLINES_SOURCE)?.setData(areas);
		// A map says nothing of what it draws; the browser tests read it here.
		(globalThis as { ventiSites?: unknown }).ventiSites = {
			signs: sites.map((site) => site.id),
			apart: sitesApart.map((site) => site.id),
			areas: areas.features.map((area) => area.properties?.id)
		};
	});

	// Draws the cables when they change, and none while the map is zoomed out.
	$effect(() => {
		if (!map || !setup) return;
		const features = cableFeatures(cables);
		setup.shapes(map, CABLES_SOURCE)?.setData(features);
		// A map says nothing of what it draws; the browser tests read it
		// here, with where each line's pylons stand on the screen now.
		const shown = map;
		(globalThis as { ventiCables?: unknown }).ventiCables = {
			lines: features.features.map((feature) => feature.properties?.id),
			dashed: features.features.filter((feature) => feature.properties?.dashed).length,
			onScreen: (id: string) =>
				cables
					.find((line) => line.id === id)
					?.points.map((point) => shown.project([point.lon, point.lat]))
					.map(({ x, y }) => ({ x, y }))
		};
	});

	// Draws the climbs of now and where thermals have been when they change.
	$effect(() => {
		if (!map || !setup) return;
		const now = climbFeatures(climbs);
		const been = flownFeatures(flown);
		setup.shapes(map, CLIMBS_SOURCE)?.setData(now);
		setup.shapes(map, FLOWN_SOURCE)?.setData(been);
		// A map says nothing of what it draws; the browser tests read it
		// here, with where each mark stands on the screen now.
		const shown = map;
		const onScreen = (at: { position: { lat: number; lon: number } } | undefined) => {
			if (!at) return undefined;
			const { x, y } = shown.project([at.position.lon, at.position.lat]);
			return { x, y };
		};
		(globalThis as { ventiAirLayers?: unknown }).ventiAirLayers = {
			climbs: now.features.map((feature) => feature.properties?.label),
			flown: been.features.length,
			climbOnScreen: (index: number) => onScreen(climbs[index]),
			flownOnScreen: (index: number) => onScreen(flown[index])
		};
	});

	// Draws the take-offs without a site when they change.
	$effect(() => {
		if (!map || !setup) return;
		const features = candidateFeatures(candidates);
		setup.shapes(map, CANDIDATES_SOURCE)?.setData(features);
		// A map says nothing of what it draws; the browser tests read it here.
		(globalThis as { ventiCandidates?: unknown }).ventiCandidates = features.features.map(
			(feature) => feature.properties?.id
		);
		// And where a ring stands on the screen now, for a test's click.
		const shown = map;
		(globalThis as { ventiCandidateOnScreen?: unknown }).ventiCandidateOnScreen = (id: string) => {
			const feature = features.features.find((one) => one.properties?.id === id);
			if (!feature) return undefined;
			const { x, y } = shown.project(
				(feature.geometry as GeoJSON.Point).coordinates as [number, number]
			);
			return { x, y };
		};
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

	// Draws the aircraft of other networks when they change.
	$effect(() => {
		if (!map || !setup) return;
		const features = otherFeatures(others);
		setup.shapes(map, OTHERS_SOURCE)?.setData(features);
		// A map says nothing of what it draws; the browser tests read it here.
		(globalThis as { ventiOthers?: unknown }).ventiOthers = features.features.map(
			(feature) => feature.properties?.id
		);
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
		const kept = untrack(() => route?.places ?? []);
		const box = boundsAround(kept.length > 0 ? [kept] : outlines);
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

	/** Where the mark of a place being chosen stands on the screen, in pixels. */
	let markAt = $state.raw<{ x: number; y: number }>();

	// The mark stands over the map's middle, which is not the screen's where
	// the navigation covers part of the map.
	$effect(() => {
		if (!map || !placing) return;
		const shown = map;
		const place = () => (markAt = shown.project(shown.getCenter()));
		place();
		shown.on('move', place);
		shown.on('resize', place);
		return () => {
			shown.off('move', place);
			shown.off('resize', place);
		};
	});

	/** The place under the mark: the middle of the map. */
	export function middle(): { lat: number; lon: number } | undefined {
		const at = map?.getCenter();
		return at && { lat: at.lat, lon: at.lng };
	}

	/** Brings a place to the middle of the map, close enough to tell a slope from its neighbour. */
	export function showPlace(at: { lat: number; lon: number }) {
		following = false;
		map?.jumpTo({ center: [at.lon, at.lat], zoom: Math.max(map.getZoom(), PLACING_ZOOM) });
	}

	/** Brings a place to the middle of the map, close enough to see the valleys around it. */
	export function showAround(at: { lat: number; lon: number }) {
		following = false;
		map?.easeTo({ center: [at.lon, at.lat], zoom: Math.max(map.getZoom(), AROUND_ZOOM) });
	}

	/**
	 * Brings a place to the middle of what a card at the map's foot leaves
	 * of it, `covered` pixels high, at the zoom the map has.
	 */
	export function showAbove(at: { lat: number; lon: number }, covered: number) {
		following = false;
		map?.easeTo({ center: [at.lon, at.lat], offset: [0, -covered / 2] });
	}

	/** One step closer or further away, for the buttons of a wide screen. */
	export function zoomBy(steps: number) {
		map?.easeTo({ zoom: map.getZoom() + steps, duration: 250 });
	}
</script>

<div class="map" bind:this={container}></div>
{#if placing && markAt}
	<div class="mark" style:left="{markAt.x}px" style:top="{markAt.y}px" aria-hidden="true"></div>
{/if}

<style>
	.map {
		position: absolute;
		inset: 0;
	}

	/* The mark of a place being chosen: a ring around a point, in the ink
	   of what is written on the map, which is light in both appearances. */
	.mark {
		position: absolute;
		width: 2.75rem;
		height: 2.75rem;
		margin: -1.375rem 0 0 -1.375rem;
		border: 0.1875rem solid var(--on-map-ink);
		border-radius: 50%;
		box-shadow:
			0 0 0 0.0625rem var(--on-map-paper),
			inset 0 0 0 0.0625rem var(--on-map-paper);
		pointer-events: none;
	}

	.mark::after {
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
</style>
