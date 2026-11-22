import { describe, expect, it } from 'vitest';
import type { PublicFlight } from '#lib/core/index.ts';
import { achievements } from './achievements';

function flight(id: string, km: number, kind?: 'freeFlight' | 'faiTriangle'): PublicFlight {
	return {
		id,
		handle: 'pilot',
		pilot: 'A Pilot',
		startUnixS: 0,
		airtimeS: 3600,
		takeoff: undefined,
		takeoffName: undefined,
		takeoffSiteId: undefined,
		trackLengthM: km * 1100,
		maxAltitudeM: 2000,
		maxClimbMs: 2,
		glider: undefined,
		score: kind
			? { kind, closed: kind !== 'freeFlight', distanceM: km * 1000, points: km }
			: undefined,
		outline: [],
		pagePath: '',
		filePath: ''
	} as unknown as PublicFlight;
}

describe('achievements', () => {
	it('names the longest, the best triangle and the best of the last days, each once', () => {
		const best = [flight('a', 80, 'faiTriangle'), flight('b', 95, 'freeFlight')];
		const recent = [flight('b', 95, 'freeFlight'), flight('c', 30, 'freeFlight')];
		expect(achievements('2026', best, recent, []).map((one) => [one.title, one.flight.id])).toEqual(
			[
				['Longest flight, 2026', 'b'],
				['Best triangle, 2026', 'a'],
				['Best of the last 14 days', 'c']
			]
		);
	});

	it('takes the newest flight when nobody flew in the last days, and skips what is not there', () => {
		const best = [flight('a', 12, 'freeFlight'), flight('b', 9, 'freeFlight')];
		const newest = [flight('a', 12, 'freeFlight'), flight('d', 4, 'freeFlight')];
		expect(achievements('', best, [], newest).map((one) => [one.title, one.flight.id])).toEqual([
			['Longest flight', 'a'],
			['Newest flight', 'd']
		]);
	});

	it('has nothing to show without flights', () => {
		expect(achievements('2026', [], [], [])).toEqual([]);
	});
});
