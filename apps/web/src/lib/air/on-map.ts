/**
 * What the map draws of what many gliders say together: a disc for each
 * climb of the last minutes, in the pack's thermal colour of its rate with
 * the core's label on it ("+2.4"), and a disc for each cell in which
 * thermals have been flown, in the thermal scale's strong step, the more
 * solid the more pilots climbed there (the core's `weight`).
 */

import type { ClimbView, FlownCellView } from '#lib/core/index.ts';
import { INK_ON_MAP, PAPER_ON_MAP, THERMAL_ON_MAP, thermalStep } from '#lib/design/palette.ts';
import { NOTHING } from '#lib/map/shapes.ts';
import type { LayerSpecification } from 'maplibre-gl';

/** The map's sources, which are also the names of the layers a click looks in. */
export const CLIMBS_SOURCE = 'air-climbs';
export const FLOWN_SOURCE = 'air-flown';
/** The climbs' labels, a layer over their discs. */
export const CLIMBS_LABELS = 'air-climbs-label';

/** How a climb or a cell is told from the others: by the place of its cell. */
export function airKey(at: { position: { lat: number; lon: number }; source?: string }): string {
	return `${at.source ?? ''}:${at.position.lat.toFixed(5)},${at.position.lon.toFixed(5)}`;
}

/** The climbs as points, each with its key, its label and the colour of its rate. */
export function climbFeatures(climbs: readonly ClimbView[] | undefined): GeoJSON.FeatureCollection {
	if (!climbs || climbs.length === 0) return NOTHING;
	return {
		type: 'FeatureCollection',
		features: climbs.map((climb) => ({
			type: 'Feature',
			geometry: { type: 'Point', coordinates: [climb.position.lon, climb.position.lat] },
			properties: {
				id: airKey(climb),
				label: climb.label,
				color: THERMAL_ON_MAP[thermalStep(climb.climbMs) - 1]
			}
		}))
	};
}

/** The cells as points, each with its key and how solid it is drawn. */
export function flownFeatures(
	cells: readonly FlownCellView[] | undefined
): GeoJSON.FeatureCollection {
	if (!cells || cells.length === 0) return NOTHING;
	return {
		type: 'FeatureCollection',
		features: cells.map((cell) => ({
			type: 'Feature',
			geometry: { type: 'Point', coordinates: [cell.position.lon, cell.position.lat] },
			properties: {
				id: airKey(cell),
				// Never so faint that a cell with few pilots cannot be seen or clicked.
				weight: 0.25 + 0.55 * Math.max(0, Math.min(1, cell.weight))
			}
		}))
	};
}

/** Where thermals have been: under the climbs of now. */
export function flownLayer(): LayerSpecification {
	return {
		id: FLOWN_SOURCE,
		type: 'circle',
		source: FLOWN_SOURCE,
		paint: {
			'circle-radius': ['interpolate', ['linear'], ['zoom'], 8, 5, 12, 11, 15, 22],
			'circle-color': THERMAL_ON_MAP[4],
			'circle-opacity': ['get', 'weight'],
			'circle-stroke-color': PAPER_ON_MAP,
			'circle-stroke-width': 1
		}
	};
}

/** The climbs of now: a disc in the colour of the rate, and the rate on it. */
export function climbLayers(font: string): LayerSpecification[] {
	return [
		{
			id: CLIMBS_SOURCE,
			type: 'circle',
			source: CLIMBS_SOURCE,
			paint: {
				'circle-radius': 15,
				'circle-color': ['get', 'color'],
				'circle-stroke-color': INK_ON_MAP,
				'circle-stroke-width': 1.5
			}
		},
		{
			id: CLIMBS_LABELS,
			type: 'symbol',
			source: CLIMBS_SOURCE,
			layout: {
				'text-field': ['get', 'label'],
				'text-font': [font],
				'text-size': 11,
				'text-allow-overlap': true,
				'text-ignore-placement': true
			},
			paint: {
				'text-color': INK_ON_MAP,
				'text-halo-color': PAPER_ON_MAP,
				'text-halo-width': 1.2
			}
		}
	];
}
