import { beforeAll, describe, expect, it } from 'vitest';
import { routePlan } from '#lib/core/route-plan.ts';
import { startCore } from '#lib/core/testing.ts';
import { closingOutline, drawnLine, faiOutlines, numberedPlaces, scoredLine } from './on-map';

beforeAll(startCore);

const WALLBERG = { lat: 47.6642, lon: 11.7967 };
const EAST = { lat: 47.6642, lon: 11.93 };
const NORTH = { lat: 47.742, lon: 11.863 };

describe('a planned route on the map', () => {
	it('is a line once it has two places, longitude first', () => {
		expect(drawnLine([WALLBERG]).features).toEqual([]);
		const line = drawnLine([WALLBERG, EAST]).features[0].geometry as GeoJSON.LineString;
		expect(line.coordinates).toEqual([
			[11.7967, 47.6642],
			[11.93, 47.6642]
		]);
	});

	it('numbers its places, and a place it comes back to keeps its first number', () => {
		const places = numberedPlaces([WALLBERG, EAST, NORTH, WALLBERG]).features;
		expect(places.map((place) => place.properties?.label)).toEqual(['1', '2', '3']);
		expect(places.map((place) => place.properties?.index)).toEqual([0, 1, 2]);
		// With the heights to leave each at, where the core has one.
		const heights = numberedPlaces([WALLBERG, EAST], [1920, undefined]).features;
		expect(heights.map((place) => place.properties?.height)).toEqual(['1920 m', '']);
	});

	it('draws the scored route, and with one leg the areas of an FAI triangle', () => {
		const leg = routePlan([WALLBERG, EAST]);
		expect(faiOutlines(leg).features).toHaveLength(2);
		expect(scoredLine(leg).features).toHaveLength(1);
		const triangle = routePlan([WALLBERG, EAST, NORTH, WALLBERG]);
		expect(faiOutlines(triangle).features).toEqual([]);
		const scored = scoredLine(triangle).features[0].geometry as GeoJSON.LineString;
		expect(scored.coordinates.length).toBeGreaterThanOrEqual(4);
		expect(scoredLine(undefined).features).toEqual([]);
	});
});

describe('a triangle’s closing circle on the map', () => {
	it('is the core’s ring around the start, for three places and for a closed triangle', () => {
		// Three places whose last comes back near the first make a triangle.
		const back = { lat: 47.7, lon: 11.8 };
		expect(routePlan([WALLBERG, EAST, NORTH]).closingCircle).toBeUndefined();
		for (const places of [
			[WALLBERG, EAST, back],
			[WALLBERG, EAST, NORTH, WALLBERG]
		]) {
			const plan = routePlan(places);
			const ring = closingOutline(plan).features[0].geometry as GeoJSON.LineString;
			expect(ring.coordinates).toHaveLength(plan.closingCircle!.outline.length);
			expect(plan.closingCircle!.words).toMatch(/^Closes within /);
		}
		expect(closingOutline(routePlan([WALLBERG, EAST])).features).toEqual([]);
	});
});
