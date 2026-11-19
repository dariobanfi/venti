/**
 * How airspace is drawn on a map: an outline and a light wash in the colour
 * of how the app treats it, and its name with its floor and ceiling. The
 * iPhone app's `AirspaceLayers.swift` draws the same.
 *
 * Magenta for airspace to stay out of, as aviation charts draw their
 * boundaries; amber for airspace that deserves attention; a thin grey line
 * for the rest. None of them is a colour a track or the wind uses. Which of
 * the three an airspace is, is the core's answer (`policy`).
 *
 * This file knows the engine's layer format and nothing of the engine
 * itself, so it is tested without a map.
 */

import type { LayerSpecification } from 'maplibre-gl';
import type { AirspacePolicy, AirspaceShape } from '#lib/core/index.ts';
import { label } from './words';

/** The map's data source that holds the airspace. */
export const AIRSPACE_SOURCE = 'airspace';

/** Below this zoom a country's airspace is a tangle; it is left off. */
export const AIRSPACE_MIN_ZOOM = 7;

/** From this zoom on, airspace is named. */
const NAMES_MIN_ZOOM = 9;

/** Between these two zooms the wash fades in. Below, only edges are drawn. */
const WASH_FROM_ZOOM = 8;
const WASH_FULL_ZOOM = 10;

/** The colour of each kind, on a map that is light in both appearances. */
export const AIRSPACE_COLORS: Readonly<Record<AirspacePolicy, string>> = {
	avoid: '#D55181',
	caution: '#C98500',
	free: '#85847F'
};

/** The least restrictive first, so that what matters most is drawn on top. */
const ORDER: readonly AirspacePolicy[] = ['free', 'caution', 'avoid'];

/** A kind as the layers' filters know it. */
export function rank(policy: AirspacePolicy): number {
	return ORDER.indexOf(policy);
}

/** The layers that draw the source, bottom to top. `font` is the label font. */
export function airspaceLayers(font: string): LayerSpecification[] {
	const layers: LayerSpecification[] = [];
	for (const policy of ORDER) {
		const only: ['==', ['get', string], number] = ['==', ['get', 'policy'], rank(policy)];
		// Airspace open to a paraglider is outlined, and not washed.
		if (policy !== 'free') {
			// On a map of a whole region the washes of airspace lying over
			// airspace would paint the wind and the valleys over: there only
			// the edges are drawn, and the wash comes in as the pilot zooms
			// to a valley.
			const full = policy === 'avoid' ? 0.1 : 0.06;
			layers.push({
				id: `airspace-wash-${rank(policy)}`,
				type: 'fill',
				source: AIRSPACE_SOURCE,
				minzoom: AIRSPACE_MIN_ZOOM,
				filter: only,
				paint: {
					'fill-color': AIRSPACE_COLORS[policy],
					'fill-opacity': [
						'interpolate',
						['linear'],
						['zoom'],
						WASH_FROM_ZOOM,
						0,
						WASH_FULL_ZOOM,
						full
					]
				}
			});
		}
		layers.push({
			id: `airspace-edge-${rank(policy)}`,
			type: 'line',
			source: AIRSPACE_SOURCE,
			minzoom: AIRSPACE_MIN_ZOOM,
			filter: only,
			layout: { 'line-join': 'round', 'line-cap': 'round' },
			paint: {
				'line-color': AIRSPACE_COLORS[policy],
				'line-width': policy === 'avoid' ? 2 : 1.2
			}
		});
	}
	layers.push({
		id: 'airspace-names',
		type: 'symbol',
		source: AIRSPACE_SOURCE,
		minzoom: NAMES_MIN_ZOOM,
		// Airspace open to a paraglider is drawn, but not named.
		filter: ['>', ['get', 'policy'], rank('free')],
		layout: {
			'text-field': ['get', 'label'],
			'text-font': [font],
			'text-size': 11
		},
		paint: {
			'text-color': '#7A1F48',
			'text-halo-color': '#ffffff',
			'text-halo-width': 1.4
		}
	});
	return layers;
}

/** The airspaces as the layers read them. */
export function airspaceFeatures(shapes: readonly AirspaceShape[]): GeoJSON.FeatureCollection {
	const ring = (points: AirspaceShape['outline']) => points.map((point) => [point.lon, point.lat]);
	return {
		type: 'FeatureCollection',
		features: shapes.map((shape) => ({
			type: 'Feature',
			geometry: {
				type: 'Polygon',
				coordinates: [ring(shape.outline), ...shape.holes.map(ring)]
			},
			properties: { id: shape.id, policy: rank(shape.policy), label: label(shape) }
		}))
	};
}
