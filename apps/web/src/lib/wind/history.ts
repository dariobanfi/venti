/**
 * The arithmetic of a station's charts: its readings in the units the
 * charts draw, the extent of the speed chart, and which reading a place on
 * a chart stands for. Drawing is in `WindChart.svelte`.
 */

import type { WindReading } from '#lib/core/index.ts';

/** One reading, in the units the charts draw. */
export interface Sample {
	timeUnixS: number;
	/** Mean wind, km/h. */
	wind: number | undefined;
	/** Gusts, km/h. */
	gusts: number | undefined;
	/** Where the wind comes from, degrees. */
	from: number | undefined;
}

const kilometresPerHour = (metresPerSecond: number | undefined) =>
	metresPerSecond === undefined ? undefined : metresPerSecond * 3.6;

export function samples(readings: readonly WindReading[]): Sample[] {
	return readings.map((reading) => ({
		timeUnixS: reading.timeUnixS,
		wind: kilometresPerHour(reading.speedMs),
		gusts: kilometresPerHour(reading.gustMs),
		from: reading.directionDeg
	}));
}

/** The position of the sample closest to a moment. Samples are in time order. */
export function nearestSample(samples: readonly Sample[], timeUnixS: number): number {
	let nearest = 0;
	for (const [index, sample] of samples.entries()) {
		if (Math.abs(sample.timeUnixS - timeUnixS) < Math.abs(samples[nearest].timeUnixS - timeUnixS)) {
			nearest = index;
		}
	}
	return nearest;
}

/**
 * The top of the speed chart, in km/h: the strongest wind or gust, widened
 * to a round number so that the line never touches the edge. Never below
 * 10, so that a calm day does not look like a storm.
 */
export function speedCeiling(samples: readonly Sample[]): number {
	const strongest = Math.max(
		0,
		...samples.flatMap((sample) => [sample.wind ?? 0, sample.gusts ?? 0])
	);
	const step = strongest < 20 ? 5 : strongest < 60 ? 10 : 20;
	return Math.max(10, Math.ceil((strongest + 0.5) / step) * step);
}

/** A run of samples that follow each other without a gap, as places on a chart. */
export type Run = { x: number; low: number; high: number }[];

/**
 * The longest gap between two readings that is still drawn as one line, in
 * seconds. Stations report every ten minutes; after half an hour of silence
 * a line across the gap would claim to know what nobody measured.
 */
const GAP_S = 30 * 60;

/**
 * Cuts the samples into runs to draw, each sample a place along the chart
 * with the mean wind and the gusts above it. A sample without a mean wind
 * ends a run, and so does a gap in time.
 */
export function runs(
	samples: readonly Sample[],
	x: (timeUnixS: number) => number,
	y: (speed: number) => number
): Run[] {
	const runs: Run[] = [];
	let current: Run = [];
	let last: number | undefined;
	for (const sample of samples) {
		const gap = last !== undefined && sample.timeUnixS - last > GAP_S;
		if ((sample.wind === undefined || gap) && current.length > 0) {
			runs.push(current);
			current = [];
		}
		if (sample.wind === undefined) continue;
		current.push({
			x: x(sample.timeUnixS),
			low: y(sample.wind),
			// Gusts are never drawn below the mean wind they belong to.
			high: y(Math.max(sample.gusts ?? sample.wind, sample.wind))
		});
		last = sample.timeUnixS;
	}
	if (current.length > 0) runs.push(current);
	return runs;
}

const place = (x: number, y: number) => `${x.toFixed(1)},${y.toFixed(1)}`;

/** The mean wind of a run as the path of a line. */
export function linePath(run: Run): string {
	return run.map((point, index) => `${index ? 'L' : 'M'}${place(point.x, point.low)}`).join('');
}

/** The span from the mean wind up to the gusts of a run, as the outline of an area. */
export function spanPath(run: Run): string {
	const up = run.map((point, index) => `${index ? 'L' : 'M'}${place(point.x, point.high)}`);
	const back = run.toReversed().map((point) => `L${place(point.x, point.low)}`);
	return `${up.join('')}${back.join('')}Z`;
}

/** The reading under the cursor in words: "18:20 · 54 km/h · gusts 93 · from W". */
export function caption(
	sample: Sample,
	time: (unixSeconds: number) => string,
	compass: (degrees: number) => string
): string {
	const parts = [time(sample.timeUnixS)];
	if (sample.wind !== undefined) parts.push(`${Math.round(sample.wind)} km/h`);
	if (sample.gusts !== undefined) parts.push(`gusts ${Math.round(sample.gusts)}`);
	if (sample.from !== undefined) parts.push(`from ${compass(sample.from)}`);
	return parts.join(' · ');
}
