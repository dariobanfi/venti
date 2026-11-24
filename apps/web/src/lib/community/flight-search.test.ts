import { describe, expect, it } from 'vitest';
import type { FlightSearch } from '#lib/core/index.ts';
import {
	circleRing,
	isSearching,
	kilometresBetween,
	only,
	placeFrom,
	sameSearch,
	searchIn,
	searchParams,
	without,
	withSearch
} from './flight-search';
import { askedIn, queryOf } from './pages';

/** Everything a search can choose at once. */
const EVERY: FlightSearch = {
	near: { lat: 47.6642, lon: 11.7967 },
	radiusKm: 10,
	area: [
		{ lat: 47.7, lon: 11.7 },
		{ lat: 47.7, lon: 11.9 },
		{ lat: 47.6, lon: 11.8 }
	],
	gliderClass: 'c',
	kind: 'faiTriangle',
	minKm: 50,
	country: 'AT'
};

const read = (query: string) => searchIn(new URLSearchParams(query));

describe('a search in the address', () => {
	it('is written in the server’s own names and read back the same', () => {
		const written = searchParams(EVERY).toString();
		expect(written).toBe(
			'near=47.6642%2C11.7967&radius_km=10&area=47.7%2C11.7%2C47.7%2C11.9%2C47.6%2C11.8' +
				'&class=en_c&kind=fai_triangle&min_km=50&country=AT'
		);
		expect(read(written)).toEqual(EVERY);
	});

	it('writes nothing for a search of everything, and reads nothing from an address without one', () => {
		expect(searchParams({}).toString()).toBe('');
		expect(searchParams(undefined).toString()).toBe('');
		expect(read('')).toEqual({});
		expect(isSearching({})).toBe(false);
		expect(isSearching({ country: 'DE' })).toBe(true);
	});

	it('leaves out what is not written as the server writes it', () => {
		expect(
			read(
				'near=47.6,abc&radius_km=10&area=47,11,48&class=en_x&kind=loop&min_km=-3&country=Austria'
			)
		).toEqual({});
		// A place without a radius is no circle, nor a radius without a place.
		expect(read('near=47.6,11.8')).toEqual({});
		expect(read('radius_km=10')).toEqual({});
		// A place off the map is none.
		expect(read('near=95,11.8&radius_km=10')).toEqual({});
		// A country is two letters in either case.
		expect(read('country=at')).toEqual({ country: 'AT' });
	});

	it('keeps a radius out of bounds for the core to refuse in words', () => {
		expect(read('near=47.6,11.8&radius_km=500')).toEqual({
			near: { lat: 47.6, lon: 11.8 },
			radiusKm: 500
		});
	});

	it('travels with the order and the days of everyone’s flights', () => {
		const asked = {
			order: 'best' as const,
			from: '2026-08-15',
			to: '2026-08-16',
			search: { gliderClass: 'b' as const, country: 'DE' }
		};
		const query = queryOf(asked);
		expect(query).toBe('?order=best&from=2026-08-15&to=2026-08-16&class=en_b&country=DE');
		expect(askedIn(new URLSearchParams(query))).toEqual(asked);
		// Without a search, the address is as it was before there were searches.
		expect(askedIn(new URLSearchParams('order=best')).search).toBeUndefined();
	});

	it('is added to an address that may already ask something', () => {
		expect(withSearch('/ranking', { country: 'AT' })).toBe('/ranking?country=AT');
		expect(withSearch('/ranking?season=2025', { country: 'AT' })).toBe(
			'/ranking?season=2025&country=AT'
		);
		expect(withSearch('/ranking', {})).toBe('/ranking');
	});
});

describe('the choices of a search', () => {
	it('are let go of one at a time, as a chip’s × does', () => {
		expect(without(EVERY, 'near')).not.toHaveProperty('near');
		expect(without(EVERY, 'near')).not.toHaveProperty('radiusKm');
		expect(without(EVERY, 'area')).not.toHaveProperty('area');
		expect(without(without(EVERY, 'gliderClass'), 'country')).toEqual({
			near: EVERY.near,
			radiusKm: 10,
			area: EVERY.area,
			kind: 'faiTriangle',
			minKm: 50
		});
	});

	it('stand alone for their chip', () => {
		expect(only(EVERY, 'near')).toEqual({ near: EVERY.near, radiusKm: 10 });
		expect(only(EVERY, 'minKm')).toEqual({ minKm: 50 });
		expect(only({}, 'area')).toEqual({});
	});

	it('are the same search whatever order they were chosen in', () => {
		expect(sameSearch({ country: 'AT', minKm: 5 }, { minKm: 5, country: 'AT' })).toBe(true);
		expect(sameSearch({ country: 'AT' }, { country: 'DE' })).toBe(false);
		expect(sameSearch(undefined, {})).toBe(true);
	});
});

describe('the circle on the map', () => {
	it('lies at the radius from its middle all round, and is closed', () => {
		const middle = { lat: 47.45, lon: 11.6 };
		const ring = circleRing(middle, 10, 36);
		expect(ring).toHaveLength(37);
		expect(ring[0]).toEqual(ring[36]);
		for (const [lon, lat] of ring) {
			expect(kilometresBetween(middle, { lat, lon })).toBeCloseTo(10, 6);
		}
		// East of the middle is further in longitude than north is in latitude.
		const east = placeFrom(middle, 10, 90);
		expect(east.lat).toBeCloseTo(47.45, 2);
		expect(east.lon).toBeGreaterThan(11.6 + 0.13);
	});
});
