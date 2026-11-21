import { describe, expect, it } from 'vitest';
import { airScale, heightLines, yOf } from './air-chart';

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
