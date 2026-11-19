import { describe, expect, it } from 'vitest';
import type { TrackPoint } from '#lib/core/index.ts';
import { altitudeRange, roundTicks, nearestIndex, scale, timeTicks } from './chart';

/** Points at 0, 2, 4, 6 and 8 seconds. */
const track: TrackPoint[] = [0, 2, 4, 6, 8].map((seconds) => ({
	lat: 47,
	lon: 11,
	altitudeM: 1000,
	seconds,
	climbMs: 0
}));

describe('the cursor', () => {
	it('snaps to the nearest moment of the track', () => {
		expect(nearestIndex(track, -5)).toBe(0);
		expect(nearestIndex(track, 0.9)).toBe(0);
		expect(nearestIndex(track, 1.1)).toBe(1);
		expect(nearestIndex(track, 4)).toBe(2);
		expect(nearestIndex(track, 6.9)).toBe(3);
		expect(nearestIndex(track, 7.1)).toBe(4);
		expect(nearestIndex(track, 100)).toBe(4);
	});

	it('has nowhere to go on an empty track', () => {
		expect(nearestIndex([], 3)).toBe(0);
	});
});

describe('the altitude axis', () => {
	it('spans whole hundreds of metres around the flight', () => {
		expect(altitudeRange({ minAltitudeM: 786, maxAltitudeM: 2117 })).toEqual([700, 2200]);
		expect(altitudeRange({ minAltitudeM: 800, maxAltitudeM: 2100 })).toEqual([800, 2100]);
	});

	it('is never flat', () => {
		expect(altitudeRange({ minAltitudeM: 1200, maxAltitudeM: 1200 })).toEqual([1200, 1300]);
	});

	it('has a few round grid lines inside the range', () => {
		expect(roundTicks(700, 2200)).toEqual([1000, 1500, 2000]);
		expect(roundTicks(1200, 1300)).toEqual([1250]);
		expect(roundTicks(400, 4300)).toEqual([1000, 2000, 3000, 4000]);
		expect(roundTicks(0, 100)).toEqual([50]);
	});
});

describe('the time axis', () => {
	const at = (hours: number, minutes: number, seconds = 0) => hours * 3600 + minutes * 60 + seconds;

	it('puts grid lines on round times of day', () => {
		// The sample flight: 29 minutes from 10:30.
		expect(timeTicks(1744, at(10, 30))).toEqual([0, 300, 600, 900, 1200, 1500]);
		// The same flight started at 10:32:10: lines at 10:35, 10:40 and so on.
		expect(timeTicks(1744, at(10, 32, 10))).toEqual([170, 470, 770, 1070, 1370, 1670]);
	});

	it('spaces them wider on a longer flight', () => {
		// Five hours from 11:10: a line every hour, on the hour.
		expect(timeTicks(5 * 3600, at(11, 10))).toEqual([3000, 6600, 10200, 13800, 17400]);
	});

	it('never draws more lines than asked for', () => {
		for (const duration of [30, 400, 1744, 4000, 9000, 20000, 40000, 100000]) {
			expect(timeTicks(duration, at(9, 17)).length).toBeLessThanOrEqual(6);
		}
	});
});

describe('a scale', () => {
	it('maps one range onto another, also downwards', () => {
		const toPixels = scale([700, 2200], [100, 0]);
		expect(toPixels(700)).toBe(100);
		expect(toPixels(2200)).toBe(0);
		expect(toPixels(1450)).toBe(50);
	});

	it('does not divide by nothing', () => {
		expect(scale([5, 5], [0, 10])(5)).toBe(0);
	});
});
