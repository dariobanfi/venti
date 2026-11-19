/**
 * Puts the pilot's airspace on a map and keeps it there as the map moves.
 * The overview map and a flight's map both use it.
 *
 * The map asks the core for the airspace of what it shows when it has come
 * to rest, not for every frame (`view.ts` decides when), and draws what
 * comes back (`layers.ts` says how).
 */

import type { AirspaceShape, Bounds } from '#lib/core/index.ts';
import type { Map } from '#lib/map/setup.ts';
import { NOTHING } from '#lib/map/shapes.ts';
import { airspaceFeatures, airspaceLayers, AIRSPACE_MIN_ZOOM, AIRSPACE_SOURCE } from './layers';
import { AirspaceInView } from './view';

/** Where a map gets its airspace from. The page's model is one. */
export interface AirspaceForMaps {
	/** Counts the changes to the pilot's airspace. */
	readonly revision: number;
	/** The airspaces that touch an area. */
	shapes(area: Bounds): Promise<AirspaceShape[]>;
}

export interface AirspaceOnMap {
	/**
	 * Looks whether the map needs other airspace than it has: after it has
	 * moved, and when the pilot's airspace has changed.
	 */
	look(): void;
}

/**
 * Adds the airspace layers to a map whose style is loaded, empty at first.
 * Call it before adding whatever is to lie on top.
 *
 * `airspace` gives the model once the app has one. `onseen` hears how many
 * airspaces are drawn on the part of the map in view whenever the map has
 * come to rest, or nothing while it is zoomed too far out to draw any.
 */
export function drawAirspace(
	module: typeof import('#lib/map/setup.ts'),
	map: Map,
	airspace: () => AirspaceForMaps | undefined,
	onseen: (count: number | undefined) => void = () => {}
): AirspaceOnMap {
	map.addSource(AIRSPACE_SOURCE, { type: 'geojson', data: NOTHING });
	const layers = airspaceLayers(module.LABEL_FONT);
	for (const layer of layers) map.addLayer(layer);
	/** The layers that draw an airspace itself, not its name. */
	const outlines = layers.filter((layer) => layer.type !== 'symbol').map((layer) => layer.id);

	const view = new AirspaceInView();
	/** Counts the questions, so that an answer overtaken by a later one is dropped. */
	let asked = 0;

	function look() {
		const model = airspace();
		if (!model || map.getZoom() < AIRSPACE_MIN_ZOOM) return;
		const shown = map.getBounds();
		const revision = model.revision;
		const area = view.wanted(
			{
				south: shown.getSouth(),
				west: shown.getWest(),
				north: shown.getNorth(),
				east: shown.getEast()
			},
			map.getZoom(),
			revision
		);
		if (!area) return;
		const question = ++asked;
		void model.shapes(area).then((shapes) => {
			// The map may have gone, or moved on, while the core was asked.
			if (question !== asked || !map.getSource(AIRSPACE_SOURCE)) return;
			module.shapes(map, AIRSPACE_SOURCE)?.setData(airspaceFeatures(shapes));
			view.drawn(area, revision);
		});
	}

	/** Says how many airspaces the map has drawn where it now rests. */
	function report() {
		if (map.getZoom() < AIRSPACE_MIN_ZOOM) {
			onseen(undefined);
			return;
		}
		const drawn = map.queryRenderedFeatures({ layers: outlines });
		onseen(new Set(drawn.map((feature) => feature.properties.id)).size);
	}

	map.on('moveend', look);
	// The engine is idle when it has drawn everything it was given.
	map.on('idle', report);
	look();
	return { look };
}
