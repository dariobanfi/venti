import { describe, expect, it } from 'vitest';
import type { TrackPoint } from '#lib/core/index.ts';
import { CLIMB_BANDS, climbBand } from '#lib/design/palette.ts';
import { runs } from './track-bands';

const [STRONG_SINK, SINK, LEVEL, CLIMB, STRONG_CLIMB] = [0, 2, 3, 4, 6];

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
	it('split at the breaks of the design pack’s vario scale', () => {
		expect(climbBand(-3.5)).toBe(STRONG_SINK);
		expect(climbBand(-3)).toBe(1);
		expect(climbBand(-1.2)).toBe(SINK);
		expect(climbBand(-0.5)).toBe(LEVEL);
		expect(climbBand(0.4)).toBe(LEVEL);
		expect(climbBand(0.5)).toBe(CLIMB);
		expect(climbBand(1.5)).toBe(5);
		expect(climbBand(3)).toBe(STRONG_CLIMB);
		expect(climbBand(12)).toBe(STRONG_CLIMB);
	});

	it('have the seven light colours of the pack, teal for sink and coral for lift', () => {
		expect(CLIMB_BANDS.map((band) => band.color)).toEqual([
			'#0A5F6C',
			'#1F93A1',
			'#86C9CC',
			'#CBC6C2',
			'#F3A682',
			'#E56954',
			'#B02A45'
		]);
		expect(CLIMB_BANDS.map((band) => band.label)).toEqual([
			'Sinking faster than 3 m/s',
			'Sinking 1.5 to 3 m/s',
			'Sinking 0.5 to 1.5 m/s',
			'Within 0.5 m/s of level',
			'Climbing 0.5 to 1.5 m/s',
			'Climbing 1.5 to 3 m/s',
			'Climbing faster than 3 m/s'
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
