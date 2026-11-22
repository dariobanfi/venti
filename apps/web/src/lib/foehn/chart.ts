/**
 * A pair's last 24 hours as a small chart: the difference in pressure over
 * time, zero in the middle, with the core's limits as lines across it.
 *
 * Everything is in hundredths of the plot's width and height, so that the
 * chart needs no measuring: `x` runs from 0 at the left to 100, `y` from 0
 * at the top to 100. Which difference is "possible" or "likely" is the
 * core's (`FoehnLimit`, `FoehnPoint.level`); nothing here decides it.
 */

import type { FoehnLimit, FoehnPoint } from '#lib/core/index.ts';

/** The span of the chart, in seconds. */
const DAY_S = 24 * 3600;

/** How much room stays beyond the largest difference or limit, as a share of it. */
const ROOM = 1.15;

export interface FoehnChartPoint {
	x: number;
	y: number;
	point: FoehnPoint;
}

export interface FoehnChartLine {
	y: number;
	limit: FoehnLimit;
}

export interface FoehnChartShape {
	/** The points that lie in the chart's 24 hours, oldest first. */
	points: FoehnChartPoint[];
	/** The line through them, as the `d` of an SVG path; empty for fewer than two. */
	path: string;
	/** The core's limits, each at its height. */
	lines: FoehnChartLine[];
	/** Where zero lies: always the middle. */
	zeroY: number;
}

/**
 * Lays out a pair's points and the limits. The chart ends at the newest
 * point and spans the 24 hours before it; it reaches as far up as down,
 * past the largest difference and the outermost limit.
 */
export function foehnChart(
	points: readonly FoehnPoint[],
	limits: readonly FoehnLimit[]
): FoehnChartShape {
	const largest = Math.max(
		0,
		...points.map((point) => Math.abs(point.differenceHpa)),
		...limits.map((limit) => Math.abs(limit.differenceHpa))
	);
	const reach = (largest || 1) * ROOM;
	const y = (hpa: number) => 50 - (hpa / reach) * 50;
	const end = Math.max(0, ...points.map((point) => point.timeUnixS));
	const start = end - DAY_S;
	const placed = points
		.filter((point) => point.timeUnixS >= start)
		.toSorted((one, other) => one.timeUnixS - other.timeUnixS)
		.map((point) => ({
			x: ((point.timeUnixS - start) / DAY_S) * 100,
			y: y(point.differenceHpa),
			point
		}));
	return {
		points: placed,
		path:
			placed.length < 2
				? ''
				: placed
						.map(({ x, y }, index) => `${index === 0 ? 'M' : 'L'}${round(x)} ${round(y)}`)
						.join(' '),
		lines: limits.map((limit) => ({ y: y(limit.differenceHpa), limit })),
		zeroY: 50
	};
}

function round(value: number): number {
	return Math.round(value * 100) / 100;
}
