/**
 * The rain radar's pictures on the map tab: for each of the radar's layers
 * that has a picture at the moment shown, an image source of its own pinned
 * by the layer's four corners, and a raster layer of its own, the radar's
 * layers in the core's order, the first lowest, and never merged into one
 * picture. A pixel is a step of rain a kilometre wide, so a picture is
 * drawn without smoothing: a step stays a step. Each is as see-through as
 * the forecast map's squares, and half as strong when its own layer's
 * newest frame is too old to be the rain of now.
 *
 * They lie where the squares lie, over them: over the ground, the relief
 * and the photographs, under the base map's cable cars, borders and names
 * and under everything else Venti draws.
 */

import type { RasterLayerSpecification } from 'maplibre-gl';
import { squareOpacity } from '#lib/forecast/thermal-layer.ts';

/** What the map's source and its layer of each of the radar's layers begin with. */
const RADAR = 'radar-';

/** The map's source and its layer for one of the radar's layers, by its place in the core's list. */
export function radarId(layer: number): string {
	return `${RADAR}${layer}`;
}

/** Whether a layer of the map is one of the radar's. */
export function isRadar(id: string): boolean {
	return id.startsWith(RADAR);
}

/**
 * The layer of the map a picture of the radar is put under: the nearest
 * of the radar's own layers above it that the map has, else what the
 * forecast's squares are put under (`under`).
 */
export function radarBefore(
	layer: number,
	onTheMap: readonly string[],
	under: string | undefined
): string | undefined {
	const above = onTheMap
		.filter(isRadar)
		.map((id) => Number(id.slice(RADAR.length)))
		.filter((place) => place > layer);
	return above.length > 0 ? radarId(Math.min(...above)) : under;
}

/** How strong an old picture is drawn beside a current one. */
const FADED = 0.5;

/** How much of the map shows through the picture, by the zoom. */
export function radarOpacity(faded: boolean) {
	return squareOpacity(faded ? FADED : 1);
}

/** The raster layer of the picture of one of the radar's layers. */
export function radarLayer(layer: number, faded: boolean): RasterLayerSpecification {
	return {
		id: radarId(layer),
		type: 'raster',
		source: radarId(layer),
		paint: {
			'raster-resampling': 'nearest',
			'raster-opacity': radarOpacity(faded),
			// Another moment's picture stands at once, not faded in: two times are never mixed.
			'raster-fade-duration': 0
		}
	};
}
