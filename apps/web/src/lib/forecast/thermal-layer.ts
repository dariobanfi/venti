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

import type {
	DataDrivenPropertyValueSpecification,
	FillLayerSpecification,
	SymbolLayerSpecification
} from 'maplibre-gl';
import type { ThermalMapKey } from '#lib/core/index.ts';
import {
	keyColor,
	LABEL_ON_MAP,
	NOT_FLYABLE_ON_MAP,
	PAPER_ON_MAP,
	THERMAL_ON_MAP
} from '#lib/design/palette.ts';

export const THERMAL_SOURCE = 'thermal-squares';

/** The arrows of the wind and the gusts, hour by hour. */
export const THERMAL_ARROWS = 'thermal-arrows';

/** The zoom from which an arrow's numbers fit beside the next square's. */
export const ARROW_LABELS_FROM_ZOOM = 10;

/** How large a forecast arrow is beside a station's: smaller, so as not to be taken for one. */
const ARROW_SIZE = 0.7;

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
			'fill-color': dayFill(),
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

/** The fill of a day's squares: the thermal scale's step as `fill`, the veil for 0. */
export function dayFill(): DataDrivenPropertyValueSpecification<string> {
	return [
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
	];
}

/**
 * The fill of an hour's squares: each square's `step` matched to the
 * colour its line of the layer's key names. A step the key lacks is left
 * clear.
 */
export function hourFill(
	legend: readonly ThermalMapKey[]
): DataDrivenPropertyValueSpecification<string> {
	if (legend.length === 0) return 'rgba(0, 0, 0, 0)';
	return [
		'match',
		['get', 'step'],
		...legend.flatMap((key) => [key.step, keyColor(key)]),
		'rgba(0, 0, 0, 0)'
	] as unknown as DataDrivenPropertyValueSpecification<string>;
}

/**
 * The arrows of an hour: the wind stations' dart, smaller and without
 * their numbers until they fit, turned where the wind blows. Calm is the
 * stations' pale dot. They lie under the stations.
 */
export function arrowsLayer(font: string): SymbolLayerSpecification {
	return {
		id: THERMAL_ARROWS,
		type: 'symbol',
		source: THERMAL_ARROWS,
		layout: {
			'icon-image': [
				'case',
				['==', ['get', 'symbol'], 'calm'],
				'wind-calm',
				['concat', 'wind-arrow-', ['get', 'band']]
			],
			'icon-rotate': ['get', 'heading'],
			'icon-rotation-alignment': 'map',
			'icon-size': ARROW_SIZE,
			'text-field': ['step', ['zoom'], '', ARROW_LABELS_FROM_ZOOM, ['get', 'label']],
			'text-font': [font],
			'text-size': 11,
			'text-anchor': 'top',
			'text-offset': [0, 0.9],
			'text-optional': true
		},
		paint: {
			'text-color': LABEL_ON_MAP,
			'text-halo-color': PAPER_ON_MAP,
			'text-halo-width': 1.4
		}
	};
}
