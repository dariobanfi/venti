/**
 * What the map draws of a route being planned: the line through its
 * places, the route as the core would score it, where a third place makes
 * an FAI triangle of the first leg, and the places with their numbers. As
 * the phones draw it.
 */

import type { Coordinate, RoutePlan } from '#lib/core/index.ts';
import { NOTHING } from '#lib/map/shapes.ts';

/** The map's sources of the route, one for each thing it draws. */
export const ROUTE_FAI = 'route-fai';
export const ROUTE_DRAWN = 'route-drawn';
export const ROUTE_SCORED = 'route-scored';
export const ROUTE_PLACES = 'route-places';

/** The layer of the places' dots, which a pointer takes hold of to move one. */
export const ROUTE_DOTS = 'route-places-dot';

const lineThrough = (places: readonly Coordinate[]): GeoJSON.FeatureCollection =>
	places.length < 2
		? NOTHING
		: {
				type: 'FeatureCollection',
				features: [
					{
						type: 'Feature',
						geometry: {
							type: 'LineString',
							coordinates: places.map((place) => [place.lon, place.lat])
						},
						properties: {}
					}
				]
			};

/** The line through the places, in order. */
export function drawnLine(places: readonly Coordinate[]): GeoJSON.FeatureCollection {
	return lineThrough(places);
}

/** The route as the core would score it: start, turnpoints and finish, on the drawn line. */
export function scoredLine(plan: RoutePlan | undefined): GeoJSON.FeatureCollection {
	return lineThrough(plan?.score?.route ?? []);
}

/** With one leg drawn, the outlines of the areas in which a third place makes an FAI triangle. */
export function faiOutlines(plan: RoutePlan | undefined): GeoJSON.FeatureCollection {
	return {
		type: 'FeatureCollection',
		features: (plan?.faiAreas ?? []).map((outline) => ({
			type: 'Feature',
			geometry: {
				type: 'LineString',
				coordinates: outline.map((place) => [place.lon, place.lat])
			},
			properties: {}
		}))
	};
}

/**
 * The places, each with its number and its place in the route, and under
 * it the height to leave it at where the core says one ("1920 m"). A place
 * the route comes back to keeps its first number and is drawn once.
 */
export function numberedPlaces(
	places: readonly Coordinate[],
	departures: readonly (number | undefined)[] = []
): GeoJSON.FeatureCollection {
	const features: GeoJSON.Feature[] = [];
	places.forEach((place, index) => {
		const again = places
			.slice(0, index)
			.some((before) => before.lat === place.lat && before.lon === place.lon);
		if (again) return;
		features.push({
			type: 'Feature',
			geometry: { type: 'Point', coordinates: [place.lon, place.lat] },
			properties: {
				index,
				label: String(index + 1),
				height: departures[index] === undefined ? '' : `${Math.round(departures[index])} m`
			}
		});
	});
	return { type: 'FeatureCollection', features };
}
