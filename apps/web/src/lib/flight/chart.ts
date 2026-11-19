/**
 * The arithmetic of the altitude chart: its extent, where its grid lines
 * go, and which moment of the flight a place on it stands for. Drawing is
 * in `AltitudeChart.svelte`.
 */

import type { FlightView, TrackPoint } from '#lib/core/index.ts';

/**
 * The position in the track closest to a moment, given in seconds since the
 * first point. A track is in time order, so this is a binary search.
 */
export function nearestIndex(track: readonly TrackPoint[], seconds: number): number {
	if (track.length === 0) return 0;
	let low = 0;
	let high = track.length - 1;
	while (low < high) {
		const middle = Math.floor((low + high) / 2);
		if (track[middle].seconds < seconds) low = middle + 1;
		else high = middle;
	}
	if (low > 0 && seconds - track[low - 1].seconds < track[low].seconds - seconds) return low - 1;
	return low;
}

/**
 * The vertical extent of the chart: the flight's altitudes widened to whole
 * hundreds of metres, and never flat.
 */
export function altitudeRange(
	flight: Pick<FlightView, 'minAltitudeM' | 'maxAltitudeM'>
): [low: number, high: number] {
	const low = Math.floor(flight.minAltitudeM / 100) * 100;
	const high = Math.ceil(flight.maxAltitudeM / 100) * 100;
	return [low, Math.max(high, low + 100)];
}

/**
 * Round values to draw grid lines at: about `wanted` of them, at steps of
 * 1, 2 or 5 times a power of ten, strictly inside the range so that no
 * label sits on the chart's edge.
 */
export function roundTicks(low: number, high: number, wanted = 3): number[] {
	const rough = (high - low) / (wanted + 1);
	const magnitude = 10 ** Math.floor(Math.log10(rough));
	const step = [1, 2, 5, 10].map((factor) => factor * magnitude).find((step) => step >= rough)!;

	const ticks: number[] = [];
	for (let tick = Math.ceil(low / step) * step; tick < high; tick += step) {
		if (tick > low) ticks.push(tick);
	}
	return ticks;
}

/** The gaps between time labels that read naturally on a clock, in seconds. */
const TIME_STEPS = [60, 120, 300, 600, 900, 1800, 3600, 7200, 10800, 21600];

/**
 * Moments to draw grid lines at, as seconds since the start of the flight:
 * at most `most` of them, on round times of day.
 *
 * `startIntoDay` is the flight's start as seconds since midnight where the
 * reader is, which is what makes "round" mean 10:35 and not 35 minutes
 * after an odd take-off time.
 */
export function timeTicks(durationS: number, startIntoDay: number, most = 6): number[] {
	const step =
		TIME_STEPS.find((step) => Math.floor(durationS / step) + 1 <= most) ?? TIME_STEPS.at(-1)!;
	const ticks: number[] = [];
	const first = Math.ceil(startIntoDay / step) * step - startIntoDay;
	for (let tick = first; tick <= durationS; tick += step) ticks.push(tick);
	return ticks;
}

/** A straight-line scale from one range of numbers onto another. */
export function scale(
	[fromLow, fromHigh]: readonly [number, number],
	[toLow, toHigh]: readonly [number, number]
): (value: number) => number {
	const span = fromHigh - fromLow || 1;
	return (value) => toLow + ((value - fromLow) / span) * (toHigh - toLow);
}
