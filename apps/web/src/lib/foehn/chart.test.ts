import { describe, expect, it } from 'vitest';
import type { FoehnLimit, FoehnPoint } from '#lib/core/index.ts';
import { foehnChart } from './chart.ts';

/** 18 November 2026, 18:50 UTC. */
const NEWEST = Date.UTC(2026, 10, 18, 18, 50) / 1000;

const LIMITS: FoehnLimit[] = [
	{ differenceHpa: 6, level: 'likely', label: 'likely' },
	{ differenceHpa: 3, level: 'possible', label: 'possible' },
	{ differenceHpa: -3, level: 'possible', label: 'possible' },
	{ differenceHpa: -6, level: 'likely', label: 'likely' }
];

function point(hoursAgo: number, differenceHpa: number): FoehnPoint {
	return { timeUnixS: NEWEST - hoursAgo * 3600, differenceHpa, level: 'none' };
}

describe('a pair’s last 24 hours as a chart', () => {
	it('has zero in the middle and the limits as far above as below', () => {
		const shape = foehnChart([point(2, 1), point(0, 2)], LIMITS);
		expect(shape.zeroY).toBe(50);
		const [likely, possible, possibleNorth, likelyNorth] = shape.lines.map((line) => line.y);
		expect(likely).toBeLessThan(possible);
		expect(possible).toBeLessThan(50);
		expect(possible + possibleNorth).toBeCloseTo(100);
		expect(likely + likelyNorth).toBeCloseTo(100);
		// Room beyond the outermost limit.
		expect(likely).toBeGreaterThan(0);
	});

	it('ends at the newest reading and spans the 24 hours before it', () => {
		const shape = foehnChart([point(30, 1), point(24, 2), point(12, 3), point(0, 4)], LIMITS);
		expect(shape.points.map((placed) => placed.x)).toEqual([0, 50, 100]);
		// A higher pressure in the south is drawn above zero.
		expect(shape.points.every((placed) => placed.y < 50)).toBe(true);
		expect(shape.path).toMatch(/^M0 [\d.]+ L50 [\d.]+ L100 [\d.]+$/);
	});

	it('reaches past a difference larger than every limit, and draws north below zero', () => {
		const shape = foehnChart([point(2, -9), point(0, -12)], LIMITS);
		for (const placed of shape.points) {
			expect(placed.y).toBeGreaterThan(50);
			expect(placed.y).toBeLessThan(100);
		}
	});

	it('draws no line through one reading, and nothing through none', () => {
		expect(foehnChart([point(0, 4)], LIMITS).path).toBe('');
		const empty = foehnChart([], LIMITS);
		expect(empty.points).toEqual([]);
		expect(empty.lines).toHaveLength(4);
		// Readings in any order come out oldest first.
		const shape = foehnChart([point(0, 4), point(6, 2)], LIMITS);
		expect(shape.points.map((placed) => placed.point.differenceHpa)).toEqual([2, 4]);
	});
});
