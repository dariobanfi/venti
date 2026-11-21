import { describe, expect, it } from 'vitest';
import type { PublicFlight } from '#lib/core/index.ts';
import { totalsOf } from './totals';

function flight(id: string, trackLengthM: number, points?: number): PublicFlight {
	return {
		id,
		handle: 'lena',
		pilot: 'Lena Probe',
		startUnixS: 0,
		airtimeS: 600,
		takeoff: undefined,
		trackLengthM,
		maxAltitudeM: 1500,
		maxClimbMs: 2,
		glider: undefined,
		score:
			points === undefined
				? undefined
				: { kind: 'freeFlight', closed: false, distanceM: trackLengthM - 1000, points },
		outline: [],
		pagePath: `/f/${id}`,
		filePath: `/f/${id}.igc`
	};
}

describe('totalsOf', () => {
	it('adds the scored distances, and the track of a flight too short to score', () => {
		const totals = totalsOf([flight('a', 12_000, 11.5), flight('b', 3_000), flight('c', 9_000, 8)]);
		expect(totals.flights).toBe(3);
		expect(totals.distanceM).toBe(11_000 + 3_000 + 8_000);
		expect(totals.best?.id).toBe('a');
	});

	it('has no best where nothing scores', () => {
		expect(totalsOf([flight('b', 3_000)]).best).toBeUndefined();
		expect(totalsOf([])).toEqual({ flights: 0, distanceM: 0, best: undefined });
	});
});
