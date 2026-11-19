import { describe, expect, it } from 'vitest';
import type { TrackPoint } from '#lib/core/index.ts';
import { CLIMB_BANDS, climbBand } from '#lib/design/palette.ts';
import { runs } from './track-bands';

const [STRONG_SINK, SINK, LEVEL, CLIMB, STRONG_CLIMB] = [0, 1, 2, 3, 4];

/** One point every two seconds with the given climb rates. */
function track(climbs: number[]): TrackPoint[] {
	return climbs.map((climbMs, index) => ({
		lat: 47 + index * 0.0001,
		lon: 11,
		altitudeM: 1000,
		seconds: index * 2,
		climbMs
	}));
}

describe('climb bands', () => {
	it('split at the rates docs/design.md gives', () => {
		expect(climbBand(-3)).toBe(STRONG_SINK);
		expect(climbBand(-2)).toBe(SINK);
		expect(climbBand(-1.2)).toBe(SINK);
		expect(climbBand(-0.5)).toBe(LEVEL);
		expect(climbBand(0.4)).toBe(LEVEL);
		expect(climbBand(0.5)).toBe(CLIMB);
		expect(climbBand(2)).toBe(STRONG_CLIMB);
		expect(climbBand(12)).toBe(STRONG_CLIMB);
	});

	it('have the five colours of docs/design.md, sink first', () => {
		expect(CLIMB_BANDS.map((band) => band.color)).toEqual([
			'#184F95',
			'#3987E5',
			'#85847F',
			'#EB6834',
			'#B8242B'
		]);
	});

	it('draw a rate that is not a number as level flight', () => {
		expect(climbBand(Number.NaN)).toBe(LEVEL);
	});
});

describe('runs of one band', () => {
	it('share their end points, so the coloured pieces join', () => {
		expect(runs(track([0, -1, -1, 3, 3, 3, -1]))).toEqual([
			{ band: SINK, from: 0, to: 2 },
			{ band: STRONG_CLIMB, from: 2, to: 5 },
			{ band: SINK, from: 5, to: 6 }
		]);
	});

	it('are none for a track too short to draw, and one for two points', () => {
		expect(runs([])).toEqual([]);
		expect(runs(track([1]))).toEqual([]);
		expect(runs(track([0, 1]))).toEqual([{ band: CLIMB, from: 0, to: 1 }]);
	});

	it('cover the whole track', () => {
		const climbs = Array.from({ length: 500 }, (_, index) => Math.sin(index / 9) * 3);
		const cut = runs(track(climbs));

		expect(cut[0].from).toBe(0);
		expect(cut.at(-1)?.to).toBe(499);
		for (let index = 1; index < cut.length; index += 1) {
			expect(cut[index].from).toBe(cut[index - 1].to);
			expect(cut[index].band).not.toBe(cut[index - 1].band);
		}
	});
});
