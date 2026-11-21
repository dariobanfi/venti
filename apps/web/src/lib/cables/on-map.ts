/**
 * What the map draws of the cables: each cable as a line along its
 * pylons, solid where its source maps its height and dashed where it does
 * not, in the pack's danger colour over the map's light casing.
 */

import type { CableLine } from '#lib/core/index.ts';
import { NOTHING } from '#lib/map/shapes.ts';

/** The map's source of the cables. */
export const CABLES_SOURCE = 'cables';

/** The layers of the lines: those with a mapped height, and those without. */
export const CABLES_MAPPED = 'cables-line';
export const CABLES_UNMAPPED = 'cables-line-unmapped';

/** The cables as lines, each with its identifier and whether it is drawn dashed. */
export function cableFeatures(lines: readonly CableLine[] | undefined): GeoJSON.FeatureCollection {
	if (!lines || lines.length === 0) return NOTHING;
	return {
		type: 'FeatureCollection',
		features: lines
			.filter((line) => line.points.length >= 2)
			.map((line) => ({
				type: 'Feature',
				geometry: {
					type: 'LineString',
					coordinates: line.points.map((point) => [point.lon, point.lat])
				},
				properties: { id: line.id, dashed: !line.heightMapped }
			}))
	};
}

/** How far a point on the screen is from a line through screen points, in pixels. */
export function pixelsFromLine(
	point: { x: number; y: number },
	line: readonly { x: number; y: number }[]
): number {
	let nearest = Infinity;
	for (let index = 1; index < line.length; index += 1) {
		const from = line[index - 1];
		const to = line[index];
		const dx = to.x - from.x;
		const dy = to.y - from.y;
		const length = dx * dx + dy * dy;
		const along =
			length === 0
				? 0
				: Math.max(0, Math.min(1, ((point.x - from.x) * dx + (point.y - from.y) * dy) / length));
		const x = from.x + along * dx;
		const y = from.y + along * dy;
		nearest = Math.min(nearest, Math.hypot(point.x - x, point.y - y));
	}
	if (line.length === 1) nearest = Math.hypot(point.x - line[0].x, point.y - line[0].y);
	return nearest;
}
