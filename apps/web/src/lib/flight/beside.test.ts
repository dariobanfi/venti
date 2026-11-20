import { describe, expect, it } from 'vitest';
import type { FlightComparison, FlightView, TrackPoint } from '#lib/core/index.ts';
import { createFormat } from '#lib/design/format.ts';
import { type Beside, besideIn, besideValue, metresApart, otherAt, otherInWords } from './beside';

const point = (seconds: number, lat = 47.66, altitudeM = 1500): TrackPoint => ({
	lat,
	lon: 11.8,
	altitudeM,
	seconds,
	climbMs: 0
});

/** A flight of ten minutes laid beside one that began two minutes before it. */
function beside(): Beside {
	const track = [0, 300, 600].map((seconds) => point(seconds, 47.66 + seconds / 100_000, 1800));
	return {
		flight: { track, durationS: 600 } as FlightView,
		name: 'Mara Muster',
		comparison: { offsetS: 120 } as FlightComparison
	};
}

describe('a flight laid beside another', () => {
	it('is named in the address and read back from it', () => {
		const query = new URLSearchParams();
		query.set('beside', besideValue({ public: 'abc' }));
		expect(query.toString()).toBe('beside=public%3Aabc');
		expect(besideIn(query)).toEqual({ public: 'abc' });
		expect(besideIn(new URLSearchParams('beside=own:7f'))).toEqual({ own: '7f' });
		expect(besideIn(new URLSearchParams('beside=elsewhere:7f'))).toBeUndefined();
		expect(besideIn(new URLSearchParams())).toBeUndefined();
	});

	it('is looked up on the first flight’s time, and is nowhere before it began or after it ended', () => {
		expect(otherAt(beside(), 60)).toBeUndefined();
		expect(otherAt(beside(), 420)?.seconds).toBe(300);
		expect(otherAt(beside(), 721)).toBeUndefined();
	});

	it('is said by its altitude and how far away it was', () => {
		const text = createFormat('en-GB', 'UTC');
		const here = point(420, 47.66);
		const there = otherAt(beside(), 420)!;
		expect(Math.round(metresApart(here, there))).toBe(334);
		expect(otherInWords(beside(), here, text)).toBe('1800 m · 330 m away');
		expect(otherInWords(beside(), point(30), text)).toBe('Not in the air then');
	});
});
