/**
 * The rain radar's picture on the map tab: one image source pinned by the
 * layer's four corners and one raster layer. A pixel is a step of rain a
 * kilometre wide, so the picture is drawn without smoothing: a step stays
 * a step. It is as see-through as the forecast map's squares, and half as
 * strong when its newest frame is too old to be the rain of now.
 *
 * It lies where the squares lie, over them: over the ground, the relief
 * and the photographs, under the base map's cable cars, borders and names
 * and under everything else Venti draws.
 */

import type { RasterLayerSpecification } from 'maplibre-gl';
import { squareOpacity } from '#lib/forecast/thermal-layer.ts';

export const RADAR = 'radar';

/** How strong an old picture is drawn beside a current one. */
const FADED = 0.5;

/** How much of the map shows through the picture, by the zoom. */
export function radarOpacity(faded: boolean) {
	return squareOpacity(faded ? FADED : 1);
}

/** The raster layer of the picture. */
export function radarLayer(faded: boolean): RasterLayerSpecification {
	return {
		id: RADAR,
		type: 'raster',
		source: RADAR,
		paint: {
			'raster-resampling': 'nearest',
			'raster-opacity': radarOpacity(faded),
			// Another frame's picture stands at once, not faded in.
			'raster-fade-duration': 0
		}
	};
}
