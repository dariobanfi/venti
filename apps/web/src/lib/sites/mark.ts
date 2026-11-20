/**
 * How flying sites are put on the map and into a list.
 *
 * What a site is, what it is called and the wind it works in are the core's
 * and arrive with the site, in words where words are shown. This file only
 * turns that into what the map engine takes, and puts a list in order.
 */

import type { LayerSpecification } from 'maplibre-gl';
import type { Bounds, SiteView } from '#lib/core/index.ts';
import { INK_ON_MAP } from '#lib/design/palette.ts';
import { pictureName } from './symbols';
import { SITES_MIN_ZOOM } from './watch';

/** The map's data source that holds the sites, and the layer that draws it. */
export const SITES_SOURCE = 'flying-sites';

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
			// Two ramps on one mountain are two signs, also where they touch.
			'icon-allow-overlap': true,
			'text-field': ['get', 'label'],
			'text-font': [font],
			'text-size': 11,
			'text-anchor': 'top',
			'text-offset': [0, 1.1],
			// A name where there is room for one.
			'text-optional': true
		},
		paint: {
			'text-color': INK_ON_MAP,
			'text-halo-color': '#ffffff',
			'text-halo-width': 1.4
		}
	};
}

/**
 * The sites as the map's symbol layer reads them. Only a name the source
 * gives is written on the map: "Take-off" under every take-off sign would
 * say nothing.
 */
export function siteFeatures(sites: readonly SiteView[]): GeoJSON.FeatureCollection {
	return {
		type: 'FeatureCollection',
		features: sites.map((site) => ({
			type: 'Feature',
			geometry: { type: 'Point', coordinates: [site.position.lon, site.position.lat] },
			properties: {
				id: site.id,
				picture: pictureName(site.kind),
				label: site.isNamed ? site.name : ''
			}
		}))
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
