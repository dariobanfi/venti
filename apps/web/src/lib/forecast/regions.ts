/**
 * What the map draws of the thermal forecast by region: each region that
 * has a level filled with its level's colour at its own opacity
 * (design/DESIGN.md, "Day levels"), its outline in the labels' paper, and
 * the thermal map's grid only outside them, over the foreland. The levels,
 * their words and the regions are the core's.
 */

import type {
	ExpressionSpecification,
	FillLayerSpecification,
	LineLayerSpecification
} from 'maplibre-gl';
import type { RegionDay, RegionLevel, RegionReadingView } from '#lib/core/index.ts';
import { LEVEL_ON_MAP, PAPER_ON_MAP } from '#lib/design/palette.ts';

/** The source of the regions' outlines, whose features are known by their `id`. */
export const REGIONS_SOURCE = 'forecast-regions';
/** The line between regions that have a level. */
export const REGIONS_EDGE = 'forecast-regions-edge';

const LEVELS = Object.keys(LEVEL_ON_MAP) as RegionLevel[];

/** A match on a region's level, kept as its feature state; `otherwise` where it has none. */
function byLevel(
	value: (level: RegionLevel) => string | number,
	otherwise: string | number
): ExpressionSpecification {
	return [
		'match',
		['coalesce', ['feature-state', 'level'], ''],
		...LEVELS.flatMap((level) => [level, value(level)]),
		otherwise
	] as unknown as ExpressionSpecification;
}

/** The fill of the regions: a level's colour at its opacity, nothing where there is no level. */
export function regionsLayer(): FillLayerSpecification {
	return {
		id: REGIONS_SOURCE,
		type: 'fill',
		source: REGIONS_SOURCE,
		paint: {
			'fill-color': byLevel((level) => LEVEL_ON_MAP[level].color, 'rgba(0, 0, 0, 0)'),
			'fill-opacity': byLevel((level) => LEVEL_ON_MAP[level].opacity, 0)
		}
	};
}

/** The edges of the regions that have a level, in the paper of the labels. */
export function regionsEdgeLayer(): LineLayerSpecification {
	return {
		id: REGIONS_EDGE,
		type: 'line',
		source: REGIONS_SOURCE,
		paint: {
			'line-color': PAPER_ON_MAP,
			'line-width': ['interpolate', ['linear'], ['zoom'], 7, 0.5, 11, 1.5],
			'line-opacity': byLevel(() => 0.9, 0)
		}
	};
}

/**
 * The regions' levels shown: those of the hour of the thermal map's line
 * at the same moment, else of the day. Nothing where the forecast by
 * region has no such hour.
 */
export function levelsShown(
	day: RegionDay | undefined,
	hourAt: number | undefined
): RegionReadingView[] {
	if (!day) return [];
	if (hourAt === undefined) return day.day;
	const hour = day.view.hours.findIndex((mark) => mark.timeUnixS === hourAt);
	return hour === -1 ? [] : (day.hours[hour] ?? []);
}

/** A square of the grid by its edges. */
interface Edges {
	south: number;
	west: number;
	north: number;
	east: number;
}

/**
 * The squares of the grid, by their number, whose middle lies in one of
 * the regions named: the squares the regions' fill stands in place of.
 */
export function cellsUnder(
	cells: readonly Edges[],
	shapes: GeoJSON.FeatureCollection | undefined,
	ids: ReadonlySet<string>
): number[] {
	if (!shapes || ids.size === 0) return [];
	const rings = shapes.features.flatMap((feature) => {
		const id = feature.properties?.id;
		if (typeof id !== 'string' || !ids.has(id)) return [];
		const geometry = feature.geometry;
		const polygons =
			geometry.type === 'Polygon'
				? [geometry.coordinates]
				: geometry.type === 'MultiPolygon'
					? geometry.coordinates
					: [];
		return polygons.map((polygon) => ({ polygon, box: boxOf(polygon[0]) }));
	});
	const under: number[] = [];
	cells.forEach((cell, index) => {
		const lon = (cell.west + cell.east) / 2;
		const lat = (cell.south + cell.north) / 2;
		const inside = rings.some(
			({ polygon, box }) =>
				lon >= box.west &&
				lon <= box.east &&
				lat >= box.south &&
				lat <= box.north &&
				inPolygon(polygon, lon, lat)
		);
		if (inside) under.push(index);
	});
	return under;
}

function boxOf(ring: GeoJSON.Position[]): Edges {
	let box = { south: Infinity, west: Infinity, north: -Infinity, east: -Infinity };
	for (const [lon, lat] of ring) {
		box = {
			south: Math.min(box.south, lat),
			west: Math.min(box.west, lon),
			north: Math.max(box.north, lat),
			east: Math.max(box.east, lon)
		};
	}
	return box;
}

/** Whether a point lies in a polygon: in its outer ring and in none of its holes. */
function inPolygon(polygon: GeoJSON.Position[][], lon: number, lat: number): boolean {
	const [outer, ...holes] = polygon;
	return inRing(outer, lon, lat) && !holes.some((hole) => inRing(hole, lon, lat));
}

/** Counts the crossings of a ray to the east. */
function inRing(ring: GeoJSON.Position[], lon: number, lat: number): boolean {
	let inside = false;
	for (let at = 0, before = ring.length - 1; at < ring.length; before = at++) {
		const [x1, y1] = ring[at];
		const [x2, y2] = ring[before];
		if (y1 > lat !== y2 > lat && lon < ((x2 - x1) * (lat - y1)) / (y2 - y1) + x1) {
			inside = !inside;
		}
	}
	return inside;
}
