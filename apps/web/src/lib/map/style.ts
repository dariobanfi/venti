/**
 * Prepares the shared map style for the browser.
 *
 * The style in `map/style.json` is the same file for every platform. It
 * leaves open where its two tile sources and its fonts live, because that
 * differs. The browser fetches tiles as the pilot moves the map, from the
 * two public services the iPhone app uses (`MapStyle.swift`), and takes its
 * label fonts from the app itself. `docs/architecture.md` has the terms.
 */

import type { RasterDEMSourceSpecification, StyleSpecification } from 'maplibre-gl';
import template from '../../../../../map/style.json';

/** The public endpoints that stand in for the project's own tile server until there is one. */
export const ONLINE = {
	/** OpenFreeMap: the base map, as a description of where its tiles are. */
	basemap: 'https://tiles.openfreemap.org/planet',
	/** Mapterhorn: elevation, for the relief shading and the terrain in 3D. */
	terrainTiles: 'https://tiles.mapterhorn.com/{z}/{x}/{y}.webp',
	/**
	 * Mapterhorn serves terrain to this zoom everywhere; finer tiles exist
	 * only for some countries. Asking for no more keeps the relief free of
	 * holes.
	 */
	terrainMaxZoom: 12,
	terrainAttribution: '<a href="https://mapterhorn.com/attribution">© Mapterhorn</a>'
} as const;

/**
 * The scheme of the address the style gives for its fonts. Nothing is
 * served under it: `fonts.ts` answers these requests from files that are
 * part of the app.
 */
export const FONT_SCHEME = 'venti-font';

/** The name of the second elevation source, the one the 3D terrain is built from. */
export const TERRAIN_3D_SOURCE = 'terrain-3d';

/** The style, pointing at the online tile services and the app's own fonts. */
export function mapStyle(): StyleSpecification {
	const style = structuredClone(template) as unknown as StyleSpecification;
	style.glyphs = `${FONT_SCHEME}://{fontstack}/{range}.pbf`;

	const basemap = style.sources.basemap as { url?: string };
	basemap.url = ONLINE.basemap;

	style.sources.terrain = elevation(style.sources.terrain as RasterDEMSourceSpecification);
	return style;
}

/**
 * The elevation source for tilting the map into 3D. It is the same tiles as
 * the relief shading under a second name: MapLibre prepares a source
 * differently for the two uses, and asks for separate sources.
 */
export function terrain3dSource(): RasterDEMSourceSpecification {
	return elevation(template.sources.terrain as RasterDEMSourceSpecification);
}

function elevation(source: RasterDEMSourceSpecification): RasterDEMSourceSpecification {
	const { url: _placeholder, ...settings } = source;
	return {
		...settings,
		tiles: [ONLINE.terrainTiles],
		maxzoom: ONLINE.terrainMaxZoom,
		attribution: ONLINE.terrainAttribution
	};
}
