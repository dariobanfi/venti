/**
 * The thermal map's squares on the map tab: one GeoJSON source and one
 * fill layer, as on the phones. The colour is the light theme's step of
 * the thermal scale the core names for each square, or the quiet veil of a
 * day that is not flyable (`palette.ts`). The squares are see-through by
 * the zoom, as the airspace's washes are: half over a region, fading to
 * 30 % in a valley, so that relief and names stay readable.
 *
 * They lie over the ground, the relief and the roads, and under the base
 * map's cable cars, borders and names, and under everything Venti draws.
 */

import type { FillLayerSpecification } from 'maplibre-gl';
import { NOT_FLYABLE_ON_MAP, THERMAL_ON_MAP } from '#lib/design/palette.ts';

export const THERMAL_SOURCE = 'thermal-squares';

/**
 * The base map's layers the squares lie under, the lowest first: the cable
 * cars (the aerial view's casing of them first), the borders. The names
 * come after them.
 */
const UNDER = ['aerial-aerialway-casing', 'aerialway', 'boundary'];

/** The fill layer of the squares. */
export function thermalLayer(): FillLayerSpecification {
	return {
		id: THERMAL_SOURCE,
		type: 'fill',
		source: THERMAL_SOURCE,
		paint: {
			'fill-color': [
				'match',
				['get', 'fill'],
				1,
				THERMAL_ON_MAP[0],
				2,
				THERMAL_ON_MAP[1],
				3,
				THERMAL_ON_MAP[2],
				4,
				THERMAL_ON_MAP[3],
				5,
				THERMAL_ON_MAP[4],
				6,
				THERMAL_ON_MAP[5],
				NOT_FLYABLE_ON_MAP
			],
			'fill-opacity': ['interpolate', ['linear'], ['zoom'], 8, 0.5, 12, 0.3],
			'fill-antialias': false
		}
	};
}

/**
 * The layer of a style the squares are put under: the lowest of the base
 * map's cable cars and borders, else its first layer of names, else none,
 * which puts them on top.
 */
export function layerAbove(layers: readonly { id: string; type: string }[]): string | undefined {
	return (
		layers.find((layer) => UNDER.includes(layer.id))?.id ??
		layers.find((layer) => layer.type === 'symbol')?.id
	);
}
