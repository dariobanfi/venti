/**
 * What the map draws of a glide's reach: the area as a filled shape, its
 * edge, dashed where a direction ran off the ground loaded (the glide
 * reaches at least that far there), and the place it starts from.
 */

import type { Coordinate, GlideReach } from '#lib/core/index.ts';
import { NOTHING } from '#lib/map/shapes.ts';

/** The map's sources of the reach, one for each thing it draws. */
export const REACH_AREA = 'reach-area';
export const REACH_EDGE = 'reach-edge';
export const REACH_START = 'reach-start';

/**
 * How much of the area's colour shows: the design pack's fill for an area
 * on the map, 12 to 16 % (`design/DESIGN.md`, Map, airspace).
 */
export const REACH_FILL = 0.14;

const at = (place: Coordinate): [number, number] => [place.lon, place.lat];

/** The area reached, a polygon of the core's ring; nothing while there is none. */
export function reachArea(reach: GlideReach | undefined): GeoJSON.FeatureCollection {
	if (!reach || reach.outline.length < 4) return NOTHING;
	return {
		type: 'FeatureCollection',
		features: [
			{
				type: 'Feature',
				geometry: { type: 'Polygon', coordinates: [reach.outline.map(at)] },
				properties: {}
			}
		]
	};
}

/**
 * The edge of the area from the end of one direction to the next, all
 * round, in runs: `dashed` where either end lies where the ground loaded
 * ends, solid elsewhere.
 */
export function reachEdge(reach: GlideReach | undefined): GeoJSON.FeatureCollection {
	const rays = reach?.rays ?? [];
	if (rays.length < 2) return NOTHING;
	const features: GeoJSON.Feature[] = [];
	let run: { dashed: boolean; line: [number, number][] } | undefined;
	rays.forEach((ray, index) => {
		const next = rays[(index + 1) % rays.length];
		const dashed = ray.end === 'notLoaded' || next.end === 'notLoaded';
		if (run?.dashed !== dashed) {
			if (run) features.push(lineOf(run));
			run = { dashed, line: [at(ray.endAt)] };
		}
		run.line.push(at(next.endAt));
	});
	if (run) features.push(lineOf(run));
	return { type: 'FeatureCollection', features };
}

function lineOf(run: { dashed: boolean; line: [number, number][] }): GeoJSON.Feature {
	return {
		type: 'Feature',
		geometry: { type: 'LineString', coordinates: run.line },
		properties: { dashed: run.dashed }
	};
}

/** The place the glide starts from. */
export function reachStart(place: Coordinate | undefined): GeoJSON.FeatureCollection {
	if (!place) return NOTHING;
	return {
		type: 'FeatureCollection',
		features: [
			{ type: 'Feature', geometry: { type: 'Point', coordinates: at(place) }, properties: {} }
		]
	};
}
