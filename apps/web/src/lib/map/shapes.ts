/**
 * Tracks as the shapes a map draws: GeoJSON, with longitude before latitude.
 */

import type { Feature, FeatureCollection, MultiLineString, Point, Position } from 'geojson';
import type { Bounds, Coordinate, FlightScore } from '#lib/core/index.ts';
import { runs } from './track-bands';

/** The map's order of coordinates. */
function position(point: { lat: number; lon: number }): Position {
	return [point.lon, point.lat];
}

/** A point of a track with the climb rate there: of a flight in the logbook, or of one being followed. */
export interface ClimbingPoint {
	lat: number;
	lon: number;
	/** Metres per second. */
	climbMs: number;
}

/**
 * A flight's track for colouring by climb: one feature per band, each
 * holding every stretch of the track flown in that band. The `band`
 * property is the band's position in `CLIMB_BANDS`.
 */
export function trackByBand(
	track: readonly ClimbingPoint[]
): FeatureCollection<MultiLineString, { band: number }> {
	const positions = track.map(position);
	const stretches = new Map<number, Position[][]>();
	for (const run of runs(track)) {
		const lines = stretches.get(run.band) ?? [];
		lines.push(positions.slice(run.from, run.to + 1));
		stretches.set(run.band, lines);
	}
	return {
		type: 'FeatureCollection',
		features: [...stretches].map(([band, coordinates]) => ({
			type: 'Feature',
			properties: { band },
			geometry: { type: 'MultiLineString', coordinates }
		}))
	};
}

/** The outlines of many flights as one shape. Outlines too short to draw are left out. */
export function outlineLines(outlines: readonly Coordinate[][]): Feature<MultiLineString> {
	return {
		type: 'Feature',
		properties: {},
		geometry: {
			type: 'MultiLineString',
			coordinates: outlines
				.filter((outline) => outline.length >= 2)
				.map((outline) => outline.map(position))
		}
	};
}

/**
 * The route a flight is scored by: start, turnpoints, finish.
 *
 * A free flight is one line through them all. A triangle's route is start,
 * three turnpoints, finish: the triangle itself is drawn closed, with the
 * way to it and the way from it.
 */
export function routeLines(score: FlightScore): Feature<MultiLineString> {
	const points = score.route.map((moment) => position(moment.position));
	const pieces =
		score.kind !== 'freeFlight' && points.length === 5
			? [
					[points[0], points[1]],
					[points[1], points[2], points[3], points[1]],
					[points[3], points[4]]
				]
			: [points];
	return {
		type: 'Feature',
		properties: {},
		geometry: { type: 'MultiLineString', coordinates: pieces }
	};
}

/** A single place. */
export function place(point: { lat: number; lon: number }): Feature<Point> {
	return {
		type: 'Feature',
		properties: {},
		geometry: { type: 'Point', coordinates: position(point) }
	};
}

/** No shape at all: what a source holds while there is nothing to draw. */
export const NOTHING: FeatureCollection = { type: 'FeatureCollection', features: [] };

/** The rectangle that contains every outline, or nothing if there are none. */
export function boundsAround(
	outlines: readonly (readonly { lat: number; lon: number }[])[]
): Bounds | undefined {
	let box: Bounds | undefined;
	for (const point of outlines.flat()) {
		box ??= { south: point.lat, west: point.lon, north: point.lat, east: point.lon };
		box.south = Math.min(box.south, point.lat);
		box.north = Math.max(box.north, point.lat);
		box.west = Math.min(box.west, point.lon);
		box.east = Math.max(box.east, point.lon);
	}
	return box;
}
