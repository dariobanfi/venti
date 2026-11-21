/**
 * A route drawn on the map before the flight, as the core works it out:
 * each leg's length, the whole, and what it would score flown as drawn.
 * A pure function of the core, called in the worker like the others.
 */

import { type Coordinate, planRoute, type RoutePlan } from './pkg/venti_core_wasm.js';

/** What the core says of a route through these places, in order. */
export function routePlan(places: readonly Coordinate[]): RoutePlan {
	return planRoute(Float64Array.from(places.flatMap((place) => [place.lat, place.lon])));
}
