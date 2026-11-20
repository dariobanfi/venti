import { describe, expect, it } from 'vitest';
import type { GroundUnderFlight } from '#lib/core/index.ts';
import { groundRuns, heightAboveGround } from './ground';

/**
 * The ground read at every `step` seconds from `from` on. The glider is as
 * many metres over it as seconds of the flight have passed.
 */
function every(step: number, moments: number, from = 0): GroundUnderFlight[] {
	return Array.from({ length: moments }, (_, index) => ({
		seconds: from + index * step,
		elevationM: 800,
		heightAboveM: from + index * step
	}));
}

describe('the height over the ground at a moment of a flight', () => {
	const ground = every(10, 7);

	it('is that of the moment the ground was read at nearest to it', () => {
		expect(heightAboveGround(0, ground)).toBe(0);
		expect(heightAboveGround(10, ground)).toBe(10);
		expect(heightAboveGround(14, ground)).toBe(10);
		expect(heightAboveGround(16, ground)).toBe(20);
		expect(heightAboveGround(60, ground)).toBe(60);
		// Half-way between two, the later one.
		expect(heightAboveGround(15, ground)).toBe(20);
	});

	it('is not known without the ground', () => {
		expect(heightAboveGround(30, [])).toBeUndefined();
	});

	it('is not known where the ground was not read: further off than the moments lie apart', () => {
		// The tile under the middle of the flight was not to be had.
		const holed = [...every(10, 20), ...every(10, 20, 600)];
		expect(heightAboveGround(30, holed)).toBe(30);
		expect(heightAboveGround(190, holed)).toBe(190);
		expect(heightAboveGround(400, holed)).toBeUndefined();
		expect(heightAboveGround(560, holed)).toBeUndefined();
		expect(heightAboveGround(600, holed)).toBe(600);
		// And before the first moment and after the last.
		expect(heightAboveGround(300, every(10, 4))).toBeUndefined();
		const late = every(10, 4, 500);
		expect(heightAboveGround(0, late)).toBeUndefined();
		expect(heightAboveGround(495, late)).toBe(500);
	});

	it('is known at the one moment the ground was read at, and for a second around it', () => {
		const once = every(10, 1, 50);
		expect(heightAboveGround(50, once)).toBe(50);
		expect(heightAboveGround(51, once)).toBe(50);
		expect(heightAboveGround(52, once)).toBeUndefined();
	});
});

describe('the ground as the chart draws it', () => {
	it('is one shape where it is known all along', () => {
		const ground = every(10, 7);
		expect(groundRuns(ground)).toEqual([ground]);
	});

	it('is no shape without the ground', () => {
		expect(groundRuns([])).toEqual([]);
	});

	it('is not drawn across a stretch it was not read along', () => {
		const before = every(10, 20);
		const after = every(10, 20, 600);
		expect(groundRuns([...before, ...after])).toEqual([before, after]);
	});

	it('is drawn across one moment that is missing', () => {
		const ground = every(10, 12).filter((moment) => moment.seconds !== 50);
		expect(groundRuns(ground)).toEqual([ground]);
	});

	it('leaves out a moment that stands alone: one height is no stretch of ground', () => {
		const before = every(10, 20);
		const alone = every(10, 1, 600);
		const after = every(10, 20, 1000);
		expect(groundRuns([...before, ...alone, ...after])).toEqual([before, after]);
	});
});
