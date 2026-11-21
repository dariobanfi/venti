/**
 * How the aircraft another network hears (the Open Glider Network) are put
 * on the map: with the wing Venti's own pilots are drawn with, in a muted
 * colour so that the two are told apart, and the core's label beside it.
 *
 * What an aircraft is, what is written beside it and whether it is a
 * paraglider or a hang glider are the core's and arrive with it. This file
 * only turns that into what the map engine takes.
 */

import type { LayerSpecification } from 'maplibre-gl';
import type { LiveOthersNetwork, OtherAircraftView } from '#lib/core/index.ts';
import { LABEL_ON_MAP, PAPER_ON_MAP } from '#lib/design/palette.ts';
import { OTHER_PICTURE } from './symbols';

/** The map's data source that holds the others, and the start of the names of its layers. */
export const OTHERS_SOURCE = 'live-others';

/** The layer of those that are not paragliders or hang gliders, drawn only close up. */
const OTHERS_POWERED = `${OTHERS_SOURCE}-more`;

/**
 * From which zoom aircraft that are not paragliders or hang gliders are
 * drawn: gliders, tow planes and airliners crowd a map further out.
 */
export const OTHERS_FROM_ZOOM = 10;

/** The two layers that draw the others: paragliders and hang gliders always, the rest close up. */
export function othersLayers(font: string): LayerSpecification[] {
	const layer = (id: string, free: boolean): LayerSpecification => ({
		id,
		type: 'symbol',
		source: OTHERS_SOURCE,
		filter: ['==', ['get', 'free'], free],
		...(free ? {} : { minzoom: OTHERS_FROM_ZOOM }),
		layout: {
			'icon-image': OTHER_PICTURE,
			'icon-rotate': ['get', 'heading'],
			// Turned against north, as Venti's own pilots are.
			'icon-rotation-alignment': 'map',
			'icon-allow-overlap': true,
			'text-field': ['get', 'label'],
			'text-font': [font],
			'text-size': 12,
			'text-anchor': 'top',
			'text-offset': [0, 1.15],
			'text-optional': true
		},
		paint: {
			'text-color': LABEL_ON_MAP,
			'text-halo-color': PAPER_ON_MAP,
			'text-halo-width': 1.6
		}
	});
	return [layer(OTHERS_POWERED, false), layer(OTHERS_SOURCE, true)];
}

/** The names of the layers a tap looks for the others in. */
export const OTHERS_LAYERS = [OTHERS_SOURCE, OTHERS_POWERED];

/**
 * The aircraft as the map's layers read them. One that sent no course
 * points north.
 */
export function otherFeatures(networks: readonly LiveOthersNetwork[]): GeoJSON.FeatureCollection {
	return {
		type: 'FeatureCollection',
		features: networks.flatMap((network) =>
			network.aircraft.map((aircraft) => ({
				type: 'Feature' as const,
				geometry: {
					type: 'Point' as const,
					coordinates: [aircraft.position.lon, aircraft.position.lat]
				},
				properties: {
					id: aircraft.id,
					heading: aircraft.courseDeg ?? 0,
					label: aircraft.label,
					free: aircraft.isFreeFlight
				}
			}))
		)
	};
}

/** Whether the map draws an aircraft at a zoom. */
export function drawnAt(aircraft: Pick<OtherAircraftView, 'isFreeFlight'>, zoom: number): boolean {
	return aircraft.isFreeFlight || zoom >= OTHERS_FROM_ZOOM;
}

/**
 * The credits of the networks with an aircraft the map draws at a zoom:
 * a network's credit stands with the map's while any of its aircraft is
 * drawn.
 */
export function drawnCredits(networks: readonly LiveOthersNetwork[], zoom: number) {
	return networks.flatMap((network) =>
		network.credit && network.aircraft.some((aircraft) => drawnAt(aircraft, zoom))
			? [network.credit]
			: []
	);
}

/** An aircraft by its identifier, with its network's credit. */
export function otherById(
	networks: readonly LiveOthersNetwork[],
	id: string
): { aircraft: OtherAircraftView; network: LiveOthersNetwork } | undefined {
	for (const network of networks) {
		const aircraft = network.aircraft.find((one) => one.id === id);
		if (aircraft) return { aircraft, network };
	}
	return undefined;
}
