import { describe, expect, it } from 'vitest';
import type { OutlookLayer } from '#lib/core/index.ts';
import { airScale, heightLines, layerBands, yOf } from './air-chart';

// The same cases as the iPhone's `ThermalTests.swift`.
describe('the scale of the air through the day', () => {
	it('reaches from a round floor under the ground to a round ceiling over the thermals', () => {
		const scale = airScale(1266, 1620, [3400, 3500, 3450]);
		expect(scale).toEqual({ floorM: 1000, ceilingM: 4000 });
		expect(heightLines(scale)).toEqual([1000, 1500, 2000, 2500, 3000, 3500, 4000]);
		expect(yOf(scale, 1000, 300)).toBeCloseTo(300);
		expect(yOf(scale, 4000, 300)).toBeCloseTo(0);
	});

	it('keeps a kilometre over the take-off, and the model’s first level above it', () => {
		expect(airScale(812, 1550, [])).toEqual({ floorM: 500, ceilingM: 3500 });
		expect(airScale(1266, 780, []).floorM).toBe(500);
		expect(airScale(1266, undefined, [])).toEqual({ floorM: 1000, ceilingM: 3000 });
		expect(airScale(1490, undefined, [], [1500, 1570, 1670, 3010, 4180])).toEqual({
			floorM: 1000,
			ceilingM: 4000
		});
		expect(heightLines(airScale(400, undefined, [4200]))).toEqual([
			0, 1000, 2000, 3000, 4000, 5000
		]);
	});
});

describe('the layers of an hour in the picture', () => {
	const layer = (bottomM: number, topM: number, inversion = false): OutlookLayer => ({
		bottomM,
		topM,
		inversion,
		fallCPer100m: inversion ? -1 : 0.3,
		words: `${inversion ? 'Inversion' : 'Stable layer'} between ${bottomM} and ${topM} m`
	});
	const scale = { floorM: 1000, ceilingM: 3500 };

	it('marks a layer between its two levels, with its kind and the core’s words', () => {
		expect(layerBands([layer(1350, 1500, true), layer(1500, 2950)], scale)).toEqual([
			{ bottomM: 1350, topM: 1500, inversion: true, words: 'Inversion between 1350 and 1500 m' },
			{ bottomM: 1500, topM: 2950, inversion: false, words: 'Stable layer between 1500 and 2950 m' }
		]);
	});

	it('cuts a layer at the picture’s edge, and keeps its words whole', () => {
		expect(layerBands([layer(3000, 4150)], scale)).toEqual([
			{ bottomM: 3000, topM: 3500, inversion: false, words: 'Stable layer between 3000 and 4150 m' }
		]);
		expect(layerBands([layer(800, 1200)], scale)[0]).toMatchObject({ bottomM: 1000, topM: 1200 });
	});

	it('leaves out a layer the picture does not reach', () => {
		expect(layerBands([layer(3500, 4150), layer(4200, 4800)], scale)).toEqual([]);
		expect(layerBands([], scale)).toEqual([]);
	});
});
