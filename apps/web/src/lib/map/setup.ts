/**
 * What every map in the app has in common. The map engine is MapLibre GL
 * JS; the iPhone app's `MapSetup.swift` does the same for MapLibre Native.
 *
 * This module pulls in the whole engine. Screens load it only once there is
 * a map to draw (`import('#lib/map/setup.ts')`), so the rest of the app does
 * not wait for it.
 */

import {
	addProtocol,
	AttributionControl,
	Map,
	setWorkerUrl,
	type CircleLayerSpecification,
	type GeoJSONSource,
	type LineLayerSpecification,
	type PaddingOptions
} from 'maplibre-gl';
import 'maplibre-gl/dist/maplibre-gl.css';
import './map.css';
import workerUrl from 'maplibre-gl/dist/maplibre-gl-worker.mjs?url';
import type { Bounds } from '#lib/core/index.ts';
import { fontLoader } from './fonts';
import { FONT_SCHEME, mapStyle, terrain3dSource, TERRAIN_3D_SOURCE } from './style';

export type { Map } from 'maplibre-gl';
export { LABEL_FONT } from './style';

let prepared = false;

/** Tells the engine, once, where its background worker is and how label fonts are found. */
function prepare(): void {
	if (prepared) return;
	setWorkerUrl(workerUrl);
	addProtocol(FONT_SCHEME, fontLoader());
	prepared = true;
}

export interface Camera {
	/** Longitude and latitude of the middle of the map. */
	center: [number, number];
	zoom: number;
}

/**
 * A map with the app's style in `container`, flat and north up. `label`
 * is what a screen reader calls it.
 */
export function createMap(container: HTMLElement, label: string, camera: Camera): Map {
	prepare();
	const map = new Map({
		container,
		style: mapStyle(),
		...camera,
		// Added below, where every screen expects it.
		attributionControl: false,
		// The map is flat until a screen tilts it: turning it shows nothing more.
		dragRotate: false,
		pitchWithRotate: false,
		touchPitch: false,
		maxPitch: 70
	});
	map.touchZoomRotate.disableRotation();
	map.keyboard.disableRotation();
	// With no option given, the control decides by the map's width whether to fold.
	map.addControl(new AttributionControl({}), 'bottom-left');
	foldCredits(map, container);
	map.getCanvas().setAttribute('aria-label', label);
	return map;
}

/**
 * The credits of the map data are written out on a wide map. On a narrow
 * one MapLibre folds them into a button, but only once the map is first
 * dragged; until then they lie across the track. Here they start folded,
 * as on the iPhone, one tap away.
 */
function foldCredits(map: Map, container: HTMLElement): void {
	const fold = () => {
		const unfolded = container.querySelector('.maplibregl-ctrl-attrib.maplibregl-compact-show');
		if (!unfolded) return;
		unfolded.classList.remove('maplibregl-compact-show');
		map.off('sourcedata', fold);
	};
	// The control decides whether to fold when the sources report who made them.
	map.on('sourcedata', fold);
}

/** Runs `draw` as soon as the map's style is there to draw on. */
export function whenStyled(map: Map, draw: () => void): void {
	if (map.isStyleLoaded()) draw();
	else map.once('load', draw);
}

/** The space, in pixels, to leave around what the camera is fitted to. */
const EDGE: Required<PaddingOptions> = { top: 28, right: 36, bottom: 28, left: 36 };

/**
 * Moves the camera so the box is in view, clear of what covers the map.
 * `covered` is how far the screen's own controls reach in from each edge.
 */
export function showBounds(map: Map, box: Bounds, covered: PaddingOptions): void {
	// A box with no extent, such as a single position, would zoom in
	// without limit.
	const margin = 0.002;
	map.setPadding(covered);
	map.fitBounds(
		[
			[box.west - margin, box.south - margin],
			[box.east + margin, box.north + margin]
		],
		{ padding: EDGE, animate: false, pitch: map.getPitch(), bearing: map.getBearing() }
	);
}

/**
 * Moves the camera so the box is in view beside something that lies over
 * the map for the moment, such as the page of what the box holds. `covered`
 * is how far that reaches in from each edge, in pixels, beyond what always
 * covers the map; the map's own padding stays as it is, so nothing jumps
 * when the page closes.
 */
export function showBeside(map: Map, box: Bounds, covered: Partial<PaddingOptions>): void {
	const margin = 0.002;
	// What stands at the end of a line has a name under it, which is wider
	// than the line: room for half of it at either side.
	const label = 24;
	map.fitBounds(
		[
			[box.west - margin, box.south - margin],
			[box.east + margin, box.north + margin]
		],
		{
			padding: {
				top: EDGE.top + (covered.top ?? 0),
				right: EDGE.right + label + (covered.right ?? 0),
				bottom: EDGE.bottom + (covered.bottom ?? 0),
				left: EDGE.left + label + (covered.left ?? 0)
			},
			// A track of a few metres is shown with the land around it.
			maxZoom: 14.5,
			duration: 600,
			pitch: map.getPitch(),
			bearing: map.getBearing()
		}
	);
}

/** A line layer with round ends and corners. */
export function line(
	id: string,
	source: string,
	color: string,
	width: number
): LineLayerSpecification {
	return {
		id,
		type: 'line',
		source,
		layout: { 'line-join': 'round', 'line-cap': 'round' },
		paint: { 'line-color': color, 'line-width': width }
	};
}

/** A round dot with a ring around it. */
export function dot(
	id: string,
	source: string,
	radius: number,
	color: string,
	ring: string
): CircleLayerSpecification {
	return {
		id,
		type: 'circle',
		source,
		paint: {
			'circle-radius': radius,
			'circle-color': color,
			'circle-stroke-color': ring,
			'circle-stroke-width': 2.5
		}
	};
}

/** The map's data source of that name, which holds shapes given as GeoJSON. */
export function shapes(map: Map, source: string): GeoJSONSource | undefined {
	return map.getSource<GeoJSONSource>(source);
}

/** How far the map leans back when it shows the terrain in 3D, in degrees. */
const TILT = 58;

/** How many zoom levels the camera steps away as it leans back. */
const STEP_BACK = 0.7;

/**
 * Tilts the map into 3D terrain, or lays it flat again.
 *
 * The terrain is built from the elevation tiles that already shade the
 * relief. Tilted, the map can also be turned and tilted further by hand;
 * flat, it stays north up as everywhere else in the app.
 */
export function setTerrain3d(map: Map, on: boolean): void {
	if (on) {
		if (!map.getSource(TERRAIN_3D_SOURCE)) map.addSource(TERRAIN_3D_SOURCE, terrain3dSource());
		map.setTerrain({ source: TERRAIN_3D_SOURCE, exaggeration: 1 });
		// A pale sky above the horizon instead of the page showing through.
		map.setSky({
			'sky-color': '#bcd9f2',
			'horizon-color': '#eef3f4',
			'fog-color': '#eef3f4',
			'sky-horizon-blend': 0.6,
			'horizon-fog-blend': 0.6,
			'fog-ground-blend': 0.2
		});
		map.dragRotate.enable();
		map.touchZoomRotate.enableRotation();
		map.touchPitch.enable();
		map.keyboard.enableRotation();
		// Leaning back brings the near ground closer: step away to keep the
		// whole track in view.
		map.easeTo({ pitch: TILT, zoom: map.getZoom() - STEP_BACK, duration: 900 });
	} else {
		map.dragRotate.disable();
		map.touchZoomRotate.disableRotation();
		map.touchPitch.disable();
		map.keyboard.disableRotation();
		map.easeTo({ pitch: 0, bearing: 0, zoom: map.getZoom() + STEP_BACK, duration: 700 });
		// The ground goes flat once the camera is level, not under it.
		map.once('moveend', () => {
			if (map.getPitch() === 0) map.setTerrain(null);
		});
	}
}
