/**
 * The ground under a flight, as the altitude chart draws it and the panel
 * reads it. How high the ground is, and how high over it the glider was,
 * is the core's (`Terrain.groundUnder`); this is what the iPhone app's
 * `FlightPanel.swift` and `AltitudeChart.swift` do with the answer.
 *
 * The ground is read at a few hundred moments of a flight, in order of
 * time, and a moment whose place no elevation tile covers is left out. So
 * there may be none at all, and there may be gaps.
 */

import type { GroundUnderFlight } from '#lib/core/index.ts';

/** How far apart the moments lie that the ground was read at, in seconds. */
function apart(ground: readonly GroundUnderFlight[]): number {
	const span = ground[ground.length - 1].seconds - ground[0].seconds;
	return Math.max(1, Math.floor(span / Math.max(ground.length - 1, 1)));
}

/**
 * The height over the ground at a moment of the flight, given in seconds
 * since its first point: that of the moment the ground was read at nearest
 * to it. Nothing where the nearest lies further off than those moments lie
 * apart: there the app has no ground.
 */
export function heightAboveGround(
	seconds: number,
	ground: readonly GroundUnderFlight[]
): number | undefined {
	if (ground.length === 0) return undefined;
	// In order of time: the first that is not earlier, by halving.
	let low = 0;
	let high = ground.length - 1;
	while (low < high) {
		const middle = Math.floor((low + high) / 2);
		if (ground[middle].seconds < seconds) low = middle + 1;
		else high = middle;
	}
	const gap = (moment: GroundUnderFlight) => Math.abs(moment.seconds - seconds);
	let nearest = ground[low];
	if (low > 0 && gap(ground[low - 1]) < gap(nearest)) nearest = ground[low - 1];
	return gap(nearest) <= apart(ground) ? nearest.heightAboveM : undefined;
}

/**
 * The ground as the stretches it is known along, each to be drawn as one
 * shape. Where more than two of the usual steps lie between two moments
 * the ground between them is not known, and no shape bridges it. A moment
 * that stands alone is no stretch.
 */
export function groundRuns(ground: readonly GroundUnderFlight[]): GroundUnderFlight[][] {
	if (ground.length === 0) return [];
	const most = 2 * apart(ground);
	const runs: GroundUnderFlight[][] = [];
	let run: GroundUnderFlight[] = [];
	for (const moment of ground) {
		const before = run.at(-1);
		if (before && moment.seconds - before.seconds > most) {
			runs.push(run);
			run = [];
		}
		run.push(moment);
	}
	runs.push(run);
	return runs.filter((stretch) => stretch.length > 1);
}
