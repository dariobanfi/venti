import { describe, expect, it } from 'vitest';
import { compassPoint, createFormat } from '#lib/design/format.ts';
import { scale } from '#lib/flight/chart.ts';
import {
	caption,
	linePath,
	nearestSample,
	runs,
	type Sample,
	samples,
	spanPath,
	speedCeiling
} from './history';
import { NOW_S, reading } from './testing';

const text = createFormat('en-GB', 'Europe/Berlin');

/** Readings ten minutes apart, as stations report. */
const tenMinutes = (count: number, given: (index: number) => Partial<Sample> = () => ({})) =>
	Array.from({ length: count }, (_, index): Sample => ({
		timeUnixS: NOW_S + index * 600,
		wind: 10,
		gusts: 20,
		from: 270,
		...given(index)
	}));

describe('readings as the charts draw them', () => {
	it('are in km/h, with the direction as it is', () => {
		const [sample] = samples([reading({ speedMs: 10, gustMs: 15, directionDeg: 240 })]);

		expect(sample.wind).toBeCloseTo(36);
		expect(sample.gusts).toBeCloseTo(54);
		expect(sample.from).toBe(240);
	});

	it('keep what a station did not measure as nothing, not as zero', () => {
		const [sample] = samples([
			reading({ speedMs: undefined, gustMs: undefined, directionDeg: undefined })
		]);

		expect(sample).toMatchObject({ wind: undefined, gusts: undefined, from: undefined });
	});
});

describe('the reading under the cursor', () => {
	const row = tenMinutes(6);

	it('is the closest in time', () => {
		expect(nearestSample(row, NOW_S + 600 + 290)).toBe(1);
		expect(nearestSample(row, NOW_S + 600 + 310)).toBe(2);
	});

	it('is the first or the last beyond the ends', () => {
		expect(nearestSample(row, 0)).toBe(0);
		expect(nearestSample(row, NOW_S * 2)).toBe(5);
	});

	it('reads as time, wind, gusts and direction', () => {
		const sample = { timeUnixS: 1_795_026_000, wind: 53.6, gusts: 92.5, from: 260 };

		// 18:20 UTC is 19:20 in Berlin in November.
		expect(caption(sample, text.time, compassPoint)).toBe('19:20 · 54 km/h · gusts 93 · from W');
	});

	it('leaves out what was not measured', () => {
		const sample = { timeUnixS: 1_795_026_000, wind: 12, gusts: undefined, from: undefined };

		expect(caption(sample, text.time, compassPoint)).toBe('19:20 · 12 km/h');
	});
});

describe('the top of the speed chart', () => {
	it('is a round number above the strongest gust', () => {
		expect(speedCeiling(tenMinutes(3, () => ({ wind: 54, gusts: 93 })))).toBe(100);
		expect(speedCeiling(tenMinutes(3, () => ({ wind: 22, gusts: 38 })))).toBe(40);
		expect(speedCeiling(tenMinutes(3, () => ({ wind: 9, gusts: 14 })))).toBe(15);
	});

	it('leaves room above a gust that is itself a round number', () => {
		expect(speedCeiling(tenMinutes(3, () => ({ wind: 20, gusts: 40 })))).toBe(50);
	});

	it('is never below 10 km/h, so a calm day does not look like a storm', () => {
		expect(speedCeiling(tenMinutes(3, () => ({ wind: 1, gusts: 2 })))).toBe(10);
		expect(speedCeiling(tenMinutes(3, () => ({ wind: undefined, gusts: undefined })))).toBe(10);
	});
});

describe('the lines of the speed chart', () => {
	const x = scale([NOW_S, NOW_S + 3000], [0, 300]);
	const y = scale([0, 40], [100, 0]);

	it('are one run through readings that follow each other', () => {
		const drawn = runs(tenMinutes(6), x, y);

		expect(drawn).toHaveLength(1);
		expect(drawn[0]).toHaveLength(6);
		expect(drawn[0][1]).toEqual({ x: 60, low: 75, high: 50 });
	});

	it('break where the station gave no wind', () => {
		const drawn = runs(
			tenMinutes(6, (index) => (index === 2 ? { wind: undefined } : {})),
			x,
			y
		);

		expect(drawn.map((run) => run.length)).toEqual([2, 3]);
	});

	it('break across a silence of more than half an hour', () => {
		const row = tenMinutes(6).filter((_, index) => index < 1 || index > 4);

		expect(runs(row, x, y).map((run) => run.length)).toEqual([1, 1]);
	});

	it('draw the gusts at the mean wind where they are unknown or below it', () => {
		const drawn = runs(
			tenMinutes(2, (index) => ({ gusts: index === 0 ? undefined : 5 })),
			x,
			y
		);

		expect(drawn[0].map((point) => point.high)).toEqual([75, 75]);
	});

	it('are a line through the mean wind and an area up to the gusts', () => {
		const [run] = runs(tenMinutes(2), x, y);

		expect(linePath(run)).toBe('M0.0,75.0L60.0,75.0');
		expect(spanPath(run)).toBe('M0.0,50.0L60.0,50.0L60.0,75.0L0.0,75.0Z');
	});
});
