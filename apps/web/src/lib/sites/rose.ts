/**
 * The geometry of the small compass on a site's page: each span of wind
 * directions as a wedge of a circle. Which directions a site works in is
 * the source's word, read by the core; this only draws them.
 */

import type { WindSpan } from '#lib/core/index.ts';

/** The width a single direction is drawn with: the sixteenth of the compass it names. */
const SINGLE_DEG = 22.5;

/**
 * The spans as SVG paths on a circle around (`centre`, `centre`), north at
 * the top. A span runs clockwise from its first direction to its second.
 */
export function wedges(spans: readonly WindSpan[], centre: number, radius: number): string[] {
	const round = (value: number) => Math.round(value * 100) / 100;
	/** The point on the circle in a compass direction, as "x y". */
	const at = (degrees: number) => {
		const angle = (degrees * Math.PI) / 180;
		return `${round(centre + radius * Math.sin(angle))} ${round(centre - radius * Math.cos(angle))}`;
	};

	return spans.map((span) => {
		const given = span.toDeg - span.fromDeg;
		if (given >= 360) {
			// Any direction: the whole disc, as two half circles.
			return `M${at(0)}A${radius} ${radius} 0 1 1 ${at(180)}A${radius} ${radius} 0 1 1 ${at(0)}Z`;
		}
		// A span through north may be written from 315 to 45.
		const width = ((given % 360) + 360) % 360;
		const [from, to] =
			width === 0
				? [span.fromDeg - SINGLE_DEG / 2, span.fromDeg + SINGLE_DEG / 2]
				: [span.fromDeg, span.fromDeg + width];
		const large = to - from > 180 ? 1 : 0;
		return `M${centre} ${centre}L${at(from)}A${radius} ${radius} 0 ${large} 1 ${at(to)}Z`;
	});
}
