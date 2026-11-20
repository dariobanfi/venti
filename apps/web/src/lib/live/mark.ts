/**
 * How the gliders of other pilots are put on the map and into a list.
 *
 * Who a glider is, where it is, whether it has landed or gone quiet and
 * what is written under it are the core's and arrive with the glider. This
 * file only turns that into what the map engine takes, and puts a list in
 * order.
 */

import type { LayerSpecification } from 'maplibre-gl';
import type { Bounds, LiveGlider } from '#lib/core/index.ts';
import { LABEL_ON_MAP } from '#lib/design/palette.ts';
import { type GliderLook, lookOf, pictureName } from './symbols';

/** The map's data source that holds the gliders, and the layer that draws it. */
export const LIVE_SOURCE = 'live-gliders';

/** The source of the followed flight's line. */
export const LIVE_TRACK_SOURCE = 'live-track';

/** The layer that draws the gliders. `font` is the label font. */
export function liveLayer(font: string): LayerSpecification {
	return {
		id: LIVE_SOURCE,
		type: 'symbol',
		source: LIVE_SOURCE,
		layout: {
			'icon-image': ['get', 'picture'],
			'icon-rotate': ['get', 'heading'],
			// Turned against north, not against the screen: the wing keeps
			// pointing where the glider flies if the map is ever rotated.
			'icon-rotation-alignment': 'map',
			// A glider is never left out for want of room.
			'icon-allow-overlap': true,
			'text-field': ['get', 'label'],
			'text-font': [font],
			'text-size': 12,
			'text-anchor': 'top',
			'text-offset': [0, 1.15],
			// The pilot's name and altitude where there is room for them.
			'text-optional': true
		},
		paint: {
			'text-color': LABEL_ON_MAP,
			'text-halo-color': '#ffffff',
			'text-halo-width': 1.6
		}
	};
}

/**
 * The gliders as the map's symbol layer reads them. A glider on the ground
 * points nowhere, and one whose app sent no course points north.
 */
export function gliderFeatures(gliders: readonly LiveGlider[]): GeoJSON.FeatureCollection {
	return {
		type: 'FeatureCollection',
		features: gliders.map((glider) => ({
			type: 'Feature',
			geometry: { type: 'Point', coordinates: [glider.position.lon, glider.position.lat] },
			properties: {
				id: glider.id,
				picture: pictureName(lookOf(glider)),
				heading: glider.landed ? 0 : (glider.courseDeg ?? 0),
				label: glider.label
			}
		}))
	};
}

const ORDER: Readonly<Record<GliderLook, number>> = { flying: 0, quiet: 1, landed: 2 };

/**
 * The gliders for a list: those that fly first, then those nothing was
 * heard of for a while, then those on the ground; pilots by name within
 * each. The server lists them by their tracks' identifiers, which is no
 * order to read.
 */
export function listed(gliders: readonly LiveGlider[]): LiveGlider[] {
	return gliders.toSorted(
		(first, second) =>
			ORDER[lookOf(first)] - ORDER[lookOf(second)] ||
			first.name.localeCompare(second.name, undefined, { sensitivity: 'base' }) ||
			first.id.localeCompare(second.id)
	);
}

/** How many of the gliders are on a part of the map. */
export function countIn(gliders: readonly LiveGlider[], region: Bounds): number {
	return gliders.filter(
		({ position }) =>
			position.lat >= region.south &&
			position.lat <= region.north &&
			position.lon >= region.west &&
			position.lon <= region.east
	).length;
}
