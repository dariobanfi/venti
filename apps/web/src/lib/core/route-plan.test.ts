import { beforeAll, describe, expect, it } from 'vitest';
import { routePlan } from './route-plan';
import { startCore } from './testing';

beforeAll(startCore);

/** The Wallberg, and two places that make an even triangle with it of some ten kilometres a side. */
const WALLBERG = { lat: 47.6642, lon: 11.7967 };
const EAST = { lat: 47.6642, lon: 11.93 };
const NORTH = { lat: 47.742, lon: 11.863 };

describe('a route drawn on the map, as the core works it out', () => {
	it('says what to do next while it has no leg', () => {
		expect(routePlan([]).words).toBe('Tap the map where the route begins.');
		const one = routePlan([WALLBERG]);
		expect(one.legs).toEqual([]);
		expect(one.score).toBeUndefined();
		expect(one.words).toBe('Tap the next place to fly to.');
	});

	it('hands the places to the core in order, latitude before longitude', () => {
		const plan = routePlan([WALLBERG, EAST]);
		expect(plan.legs.map((leg) => leg.words)).toEqual(['10.0 km E']);
		expect(plan.score?.kind).toBe('freeFlight');
		// With one leg, where a third place makes an FAI triangle of it.
		expect(plan.faiAreas.length).toBe(2);
	});

	it('finds a triangle in a route that comes back to its start', () => {
		const plan = routePlan([WALLBERG, EAST, NORTH, WALLBERG]);
		expect(plan.legs).toHaveLength(3);
		expect(plan.score?.kind).toBe('faiTriangle');
		expect(plan.score?.closed).toBe(true);
		expect(plan.score?.name).toBe('Closed FAI triangle');
		expect(plan.words).toContain('Flown like this it scores as a closed FAI triangle');
		expect(plan.faiAreas).toEqual([]);
	});
});
