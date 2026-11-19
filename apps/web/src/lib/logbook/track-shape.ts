/**
 * The shape of a flight as a small drawing. The iPhone app's `TrackShape`
 * does the same.
 */

import type { Coordinate } from '#lib/core/index.ts';

/**
 * The outline scaled to fit a box of `width` by `height` without
 * distortion, north up, leaving `inset` free on every side. The result is
 * the `points` of an SVG polyline. An outline of fewer than two points has
 * no shape.
 */
export function outlinePoints(
	outline: readonly Coordinate[],
	width: number,
	height: number,
	inset: number
): string | undefined {
	if (outline.length < 2) return undefined;

	// Degrees of longitude are narrower than degrees of latitude by the
	// cosine of the latitude. Over one flight a single factor is enough.
	const narrowing = Math.cos((outline[0].lat * Math.PI) / 180);
	const xs = outline.map((point) => point.lon * narrowing);
	const ys = outline.map((point) => point.lat);
	const [west, east] = [Math.min(...xs), Math.max(...xs)];
	const [south, north] = [Math.min(...ys), Math.max(...ys)];

	const scale = Math.min(
		(width - 2 * inset) / Math.max(east - west, 1e-9),
		(height - 2 * inset) / Math.max(north - south, 1e-9)
	);
	const centreX = width / 2 - ((west + east) / 2) * scale;
	const centreY = height / 2 + ((south + north) / 2) * scale;

	return outline
		.map((_, index) => {
			const x = centreX + xs[index] * scale;
			const y = centreY - ys[index] * scale;
			return `${x.toFixed(1)},${y.toFixed(1)}`;
		})
		.join(' ');
}
