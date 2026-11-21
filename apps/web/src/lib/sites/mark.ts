/**
 * How flying sites are put on the map and into a list.
 *
 * What a site is, what it is called and the wind it works in are the core's
 * and arrive with the site, in words where words are shown. This file only
 * turns that into what the map engine takes, and puts a list in order.
 */

import type { LayerSpecification } from 'maplibre-gl';
import type { Bounds, SiteView } from '#lib/core/index.ts';
import { INK_ON_MAP, MUTED_INK_ON_MAP, PAPER_ON_MAP } from '#lib/design/palette.ts';
import { pictureName } from './symbols';
import { SITES_MIN_ZOOM } from './watch';

/** The map's data source that holds the sites, and the layer that draws it. */
export const SITES_SOURCE = 'flying-sites';

/**
 * The source that holds the outlines of the sites mapped as areas, and the
 * layers that draw them under the signs. Their names begin as the signs'
 * layer does, so that the map's "Sites" chip switches all three.
 */
export const SITE_OUTLINES_SOURCE = 'flying-sites-outlines';
export const SITE_OUTLINE_LAYERS = ['flying-sites-outline-fill', 'flying-sites-outline-line'];

/**
 * From this zoom on a field is drawn as its outline, coming in over one
 * step of zoom: at 47° north a pixel is 13 m here and 6 m a step closer,
 * so that a landing field of a hundred metres grows past the 24 px of its
 * sign between the two. Further out the outline would hide under the pin.
 */
export const OUTLINES_FROM_ZOOM = 13;
const OUTLINES_FULL_ZOOM = 14;

/**
 * How an area is drawn on the map, as `design/DESIGN.md` has it for
 * airspace, the one area it describes: a 2 px outline and a fill of 12 %,
 * the faint end of its 12 to 16 %.
 */
const OUTLINE_WIDTH = 2;
const AREA_FILL = 0.12;

/**
 * The layers that draw a site's area: a faint fill and a thin outline in
 * the colour of the sign, the ink of its pin, and muted as the pin is for
 * a take-off that stands apart.
 */
export function siteOutlineLayers(): LayerSpecification[] {
	const [fill, line] = SITE_OUTLINE_LAYERS;
	const colour: ['case', ['get', string], string, string] = [
		'case',
		['get', 'apart'],
		MUTED_INK_ON_MAP,
		INK_ON_MAP
	];
	const comingIn = (full: number): ['interpolate', ['linear'], ['zoom'], ...number[]] => [
		'interpolate',
		['linear'],
		['zoom'],
		OUTLINES_FROM_ZOOM,
		0,
		OUTLINES_FULL_ZOOM,
		full
	];
	return [
		{
			id: fill,
			type: 'fill',
			source: SITE_OUTLINES_SOURCE,
			minzoom: OUTLINES_FROM_ZOOM,
			paint: { 'fill-color': colour, 'fill-opacity': comingIn(AREA_FILL) }
		},
		{
			id: line,
			type: 'line',
			source: SITE_OUTLINES_SOURCE,
			minzoom: OUTLINES_FROM_ZOOM,
			layout: { 'line-join': 'round', 'line-cap': 'round' },
			paint: {
				'line-color': colour,
				'line-width': OUTLINE_WIDTH,
				'line-opacity': comingIn(1)
			}
		}
	];
}

/**
 * The outlines of the sites their source maps as areas, landings and
 * take-offs alike, as the layers above read them. A site mapped as a point
 * has none. `apart` are the take-offs whose source does not say what the
 * filter asks.
 */
export function siteOutlines(
	sites: readonly SiteView[],
	apart: readonly SiteView[] = []
): GeoJSON.FeatureCollection {
	const area = (isApart: boolean) => (site: SiteView) =>
		// A ring closes on its first point: fewer than four is no area.
		site.outline.length < 4
			? []
			: [
					{
						type: 'Feature' as const,
						geometry: {
							type: 'Polygon' as const,
							coordinates: [site.outline.map((point) => [point.lon, point.lat])]
						},
						properties: { id: site.id, apart: isApart }
					}
				];
	return {
		type: 'FeatureCollection',
		features: [...apart.flatMap(area(true)), ...sites.flatMap(area(false))]
	};
}

/** The layer that draws the source. `font` is the label font. */
export function sitesLayer(font: string): LayerSpecification {
	return {
		id: SITES_SOURCE,
		type: 'symbol',
		source: SITES_SOURCE,
		// From the zoom at which they can be told apart.
		minzoom: SITES_MIN_ZOOM,
		layout: {
			'icon-image': ['get', 'picture'],
			// The pin's tip on the site.
			'icon-anchor': 'bottom',
			// Two ramps on one mountain are two pins, also where they touch.
			'icon-allow-overlap': true,
			'text-field': ['get', 'label'],
			'text-font': [font],
			'text-size': 11,
			'text-anchor': 'top',
			// The name under the pin's tip.
			'text-offset': [0, 0.2],
			// A name where there is room for one.
			'text-optional': true,
			// The sites a filter leaves lie over those that stand apart.
			'symbol-sort-key': ['case', ['get', 'apart'], 0, 1]
		},
		paint: {
			'text-color': INK_ON_MAP,
			'text-halo-color': PAPER_ON_MAP,
			'text-halo-width': 1.4
		}
	};
}

/**
 * The sites as the map's symbol layer reads them. Only a name the source
 * gives is written on the map: "Take-off" under every take-off sign would
 * say nothing.
 *
 * `apart` are the take-offs whose source does not say what the filter
 * asks: they stay on the map, as the muted pin and without a name, so that
 * they are told from the ones that fit.
 */
export function siteFeatures(
	sites: readonly SiteView[],
	apart: readonly SiteView[] = []
): GeoJSON.FeatureCollection {
	const sign = (isApart: boolean) => (site: SiteView) => ({
		type: 'Feature' as const,
		geometry: { type: 'Point' as const, coordinates: [site.position.lon, site.position.lat] },
		properties: {
			id: site.id,
			picture: pictureName(site.kind, isApart),
			label: site.isNamed && !isApart ? site.name : '',
			apart: isApart
		}
	});
	return {
		type: 'FeatureCollection',
		features: [...apart.map(sign(true)), ...sites.map(sign(false))]
	};
}

/**
 * The sites for a list: those the source names first, by name; then the
 * rest by what they are. `wanted` is what the pilot typed to find one.
 */
export function listed(sites: readonly SiteView[], wanted = ''): SiteView[] {
	const part = wanted.trim().toLocaleLowerCase();
	return sites
		.filter((site) => site.name.toLocaleLowerCase().includes(part))
		.toSorted((first, second) =>
			first.isNamed === second.isNamed
				? first.name.localeCompare(second.name, undefined, { sensitivity: 'base' })
				: first.isNamed
					? -1
					: 1
		);
}

/** How many of the sites lie on a part of the map. */
export function countIn(sites: readonly SiteView[], region: Bounds): number {
	return sites.filter(
		({ position }) =>
			position.lat >= region.south &&
			position.lat <= region.north &&
			position.lon >= region.west &&
			position.lon <= region.east
	).length;
}

/**
 * Whether flights can have taken off from a site, so that its page leads to
 * them: every site but one that its source calls a landing and nothing
 * else. A site whose source says nothing of its use may be a take-off.
 */
export function flownFrom(site: Pick<SiteView, 'uses'>): boolean {
	return !(site.uses.length > 0 && site.uses.every((use) => use === 'landing'));
}
