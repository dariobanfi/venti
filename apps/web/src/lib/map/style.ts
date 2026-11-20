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
import { ONLINE } from './online';

export { ONLINE };

/**
 * The scheme of the address the style gives for its fonts. Nothing is
 * served under it: `fonts.ts` answers these requests from files that are
 * part of the app.
 */
export const FONT_SCHEME = 'venti-font';

/** The font the style sets its labels in, for layers the app adds itself. */
export const LABEL_FONT = 'noto-sans-regular';

/** What stands in the shared style for the address of the Venti server an app talks to. */
const VENTI_SERVER = '__VENTI_SERVER__';

/** What stands in the shared style for the domain an app names itself by to a provider that asks who is calling. */
const APP_DOMAIN = '__APP_DOMAIN__';

/** Where the page is, as far as the style needs it. */
export interface Where {
	/** The page's origin, "https://venti.example". */
	origin?: string;
	/** The page's host name, "venti.example". */
	hostname?: string;
}

/**
 * The domain a web page names itself by: the host it is served from, if
 * that is a domain. A page opened from this machine or by a bare address
 * has none, and what needs one is then left out of the style.
 */
export function appDomain(hostname: string | undefined): string | undefined {
	if (!hostname?.includes('.') || /^[\d.]+$/u.test(hostname)) return undefined;
	return hostname;
}

/** The name of the second elevation source, the one the 3D terrain is built from. */
export const TERRAIN_3D_SOURCE = 'terrain-3d';

/** The layers that belong to one of the map's two views only, as the shared style marks them. */
function layersOf(view: 'aerial' | 'drawn'): readonly string[] {
	return (template.layers as { id: string; metadata?: Record<string, string> }[])
		.filter((layer) => layer.metadata?.['venti:view'] === view)
		.map((layer) => layer.id);
}

/**
 * The layers of the aerial view: photographs from above, and what is drawn
 * on them in place of the drawn map's roads. The shared style keeps them
 * hidden, and nothing of the photographs is fetched until they are shown.
 */
export const AERIAL_LAYERS = layersOf('aerial');

/** The layers of the drawn map that the aerial view does without: its roads, tracks and paths. */
export const DRAWN_LAYERS = layersOf('drawn');

/**
 * The layers of the thermal map, which a chip switches on and off, as the
 * shared style marks them. A map has them only where the page has a
 * domain to name to the map's author: see {@link mapStyle}.
 */
export const THERMAL_LAYERS: readonly string[] = (
	template.layers as { id: string; metadata?: Record<string, string> }[]
)
	.filter((layer) => layer.metadata?.['venti:layer'] === 'thermals')
	.map((layer) => layer.id);

/**
 * The style, pointing at the online tile services and the app's own fonts.
 * `where` is the page's address, for what the style asks of Venti itself
 * and for what has to be told who is asking.
 */
export function mapStyle(where: Where = globalThis.location ?? {}): StyleSpecification {
	const style = structuredClone(template) as unknown as StyleSpecification;
	style.glyphs = `${FONT_SCHEME}://{fontstack}/{range}.pbf`;

	const basemap = style.sources.basemap as { url?: string };
	basemap.url = ONLINE.basemap;

	style.sources.terrain = elevation(style.sources.terrain as RasterDEMSourceSpecification);

	// What comes through Venti, Austria's photographs cut at the border, is
	// asked of the server the app itself came from. What has to be told
	// who is asking, the thermal map, is told the page's domain; a page
	// without one leaves that source and its layers out.
	const values: Record<string, string | undefined> = {
		[VENTI_SERVER]: where.origin ?? '',
		[APP_DOMAIN]: appDomain(where.hostname)
	};
	for (const [name, source] of Object.entries(style.sources)) {
		const tiles = (source as { tiles?: string[] }).tiles;
		if (!tiles) continue;
		const named = Object.keys(values).filter((placeholder) =>
			tiles.some((address) => address.includes(placeholder))
		);
		if (named.some((placeholder) => values[placeholder] === undefined)) {
			delete style.sources[name];
			style.layers = style.layers.filter((layer) => (layer as { source?: string }).source !== name);
			continue;
		}
		(source as { tiles?: string[] }).tiles = tiles.map((address) =>
			named.reduce(
				(filled, placeholder) => filled.replace(placeholder, values[placeholder] ?? ''),
				address
			)
		);
	}
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
