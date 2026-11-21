import { describe, expect, it } from 'vitest';
import { boxAround, offerFor, todayBox, utcOffsetS } from './takeoffs.svelte';

describe('the box asked about', () => {
	it('holds every place, and nothing without one', () => {
		expect(boxAround([])).toBeUndefined();
		expect(
			boxAround([
				{ lat: 47.3, lon: 11.1 },
				{ lat: 47.2, lon: 11.3 }
			])
		).toEqual({ south: 47.2, west: 11.1, north: 47.3, east: 11.3 });
	});

	it('on Today holds the place and its take-offs near it, not a kept one far away', () => {
		const middle = { lat: 47.5, lon: 11.5 };
		expect(
			todayBox(
				[
					{ lat: 47.6, lon: 11.4 },
					{ lat: 45.9, lon: 7.6 }
				],
				middle
			)
		).toEqual({ south: 47.5, west: 11.4, north: 47.6, east: 11.5 });
	});
});

describe('the offer of a candidate', () => {
	it('is found by its place', () => {
		const offer = { candidate: { position: { lat: 47.2612, lon: 10.9312 } } };
		const offers = [offer] as never[];
		expect(offerFor(offers, { position: { lat: 47.2612, lon: 10.9312 } })).toBe(offer);
		expect(offerFor(offers, { position: { lat: 47.2613, lon: 10.9312 } })).toBeUndefined();
	});
});

describe("the browser's clock", () => {
	it('is ahead of UTC by the negative of what the date says of its zone', () => {
		const at = new Date();
		expect(utcOffsetS(at)).toBe(-at.getTimezoneOffset() * 60);
	});
});
