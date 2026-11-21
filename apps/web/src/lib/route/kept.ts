/**
 * The route the pilot is drawing on the map, as this browser remembers it
 * until it is cleared: its places in order, each with the name of the site
 * it was taken from where it was, and whether the planner was open. A
 * convenience of the browser, like the map's other choices: kept in
 * `localStorage`, and nothing is drawn wherever that cannot be read.
 */

import type { Coordinate } from '#lib/core/index.ts';

/** A place of a route: where, and the site's name where a site's sign was tapped. */
export interface RoutePlace extends Coordinate {
	name?: string;
}

/** The route as kept: its places, and whether the planner is open on the map. */
export interface KeptRoute {
	places: RoutePlace[];
	open: boolean;
}

const KEY = 'venti.route-plan';

export const NO_ROUTE: KeptRoute = { places: [], open: false };

function isPlace(value: unknown): value is RoutePlace {
	if (typeof value !== 'object' || value === null) return false;
	const { lat, lon, name } = value as Record<string, unknown>;
	return (
		typeof lat === 'number' &&
		typeof lon === 'number' &&
		Number.isFinite(lat) &&
		Number.isFinite(lon) &&
		(name === undefined || typeof name === 'string')
	);
}

/** The route this browser keeps. Nothing where it keeps none, or what it keeps is not a route. */
export function keptRoute(storage: Pick<Storage, 'getItem'> | undefined): KeptRoute {
	try {
		const kept: unknown = JSON.parse(storage?.getItem(KEY) ?? 'null');
		if (typeof kept !== 'object' || kept === null) return NO_ROUTE;
		const { places, open } = kept as Record<string, unknown>;
		if (!Array.isArray(places) || !places.every(isPlace)) return NO_ROUTE;
		return {
			places: places.map(({ lat, lon, name }) => (name ? { lat, lon, name } : { lat, lon })),
			open: open === true
		};
	} catch {
		return NO_ROUTE;
	}
}

/** Keeps the route; a route of no places and closed is forgotten. Nothing happens where the browser keeps nothing. */
export function keepRoute(
	storage: Pick<Storage, 'setItem' | 'removeItem'> | undefined,
	route: KeptRoute
): void {
	try {
		if (route.places.length === 0 && !route.open) storage?.removeItem(KEY);
		else storage?.setItem(KEY, JSON.stringify(route));
	} catch {
		// A private window, or no storage: the route lasts for the visit.
	}
}
