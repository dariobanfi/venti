import { beforeAll, describe, expect, it } from 'vitest';
import type { Coordinate } from '#lib/core/index.ts';
import { routePlan } from '#lib/core/route-plan.ts';
import { startCore } from '#lib/core/testing.ts';
import { keepRoute, keptRoute, NO_ROUTE } from './kept';
import { RoutePlanner } from './planner.svelte';

beforeAll(startCore);

function memory() {
	const kept = new Map<string, string>();
	return {
		kept,
		getItem: (key: string) => kept.get(key) ?? null,
		setItem: (key: string, value: string) => void kept.set(key, value),
		removeItem: (key: string) => void kept.delete(key)
	};
}

const WALLBERG = { lat: 47.6642, lon: 11.7967, name: 'Wallberg' };
const EAST = { lat: 47.6642, lon: 11.93 };
const NORTH = { lat: 47.742, lon: 11.863 };

/** The core itself, as the worker would answer. */
const ask = async (places: Coordinate[]) => routePlan(places);
const settled = () => new Promise((resolve) => setTimeout(resolve, 0));

describe('the route the browser keeps', () => {
	it('is nothing until one is kept, and nothing where what is kept is not a route', () => {
		const storage = memory();
		expect(keptRoute(storage)).toEqual(NO_ROUTE);
		expect(keptRoute(undefined)).toEqual(NO_ROUTE);
		storage.setItem('venti.route-plan', '{"places":[{"lat":"north"}],"open":true}');
		expect(keptRoute(storage)).toEqual(NO_ROUTE);
		storage.setItem('venti.route-plan', 'not json');
		expect(keptRoute(storage)).toEqual(NO_ROUTE);
	});

	it('keeps the places with their sites’ names and whether the planner is open', () => {
		const storage = memory();
		keepRoute(storage, { places: [WALLBERG, EAST], open: true });
		expect(keptRoute(storage)).toEqual({ places: [WALLBERG, EAST], open: true });
		// Cleared and closed, it is forgotten.
		keepRoute(storage, { places: [], open: false });
		expect(storage.kept.size).toBe(0);
	});

	it('lets a browser that keeps nothing draw a route for the visit', () => {
		const refusing = {
			getItem: () => {
				throw new Error('no storage');
			},
			setItem: () => {
				throw new Error('no storage');
			},
			removeItem: () => {}
		};
		expect(keptRoute(refusing)).toEqual(NO_ROUTE);
		expect(() => keepRoute(refusing, { places: [EAST], open: true })).not.toThrow();
	});
});

describe('the route planner', () => {
	it('asks the core after every place and keeps the route until it is cleared', async () => {
		const storage = memory();
		const planner = new RoutePlanner(storage, ask);
		await settled();
		expect(planner.open).toBe(false);
		expect(planner.plan?.words).toBe('Tap the map where the route begins.');

		planner.startAt(WALLBERG);
		planner.add(EAST);
		planner.add(NORTH);
		await settled();
		expect(planner.plan?.legs).toHaveLength(2);
		expect(planner.canClose).toBe(true);

		planner.backToStart();
		await settled();
		expect(planner.places).toHaveLength(4);
		expect(planner.canClose).toBe(false);
		expect(planner.plan?.score?.name).toBe('Closed FAI triangle');

		// A reload: the same route, open, with the site's name.
		const again = new RoutePlanner(storage, ask);
		await settled();
		expect(again.open).toBe(true);
		expect(again.places[0].name).toBe('Wallberg');
		expect(again.plan?.legs).toHaveLength(3);

		again.undo();
		await settled();
		expect(again.places).toHaveLength(3);
		expect(keptRoute(storage).places).toHaveLength(3);

		again.clear();
		again.close();
		await settled();
		expect(again.plan?.legs).toEqual([]);
		expect(storage.kept.size).toBe(0);
	});

	it('moves a place, and the start with the finish of a closed route', async () => {
		const planner = new RoutePlanner(memory(), ask);
		planner.startAt(WALLBERG);
		planner.add(EAST);
		planner.add(NORTH);
		planner.backToStart();
		const there = { lat: 47.65, lon: 11.8 };
		planner.move(0, there);
		expect(planner.places[0]).toEqual(there);
		expect(planner.places[3]).toEqual(there);
		planner.move(1, NORTH);
		expect(planner.places[1]).toEqual(NORTH);
		// No such place: nothing changes.
		planner.move(9, there);
		expect(planner.places).toHaveLength(4);
	});

	it('takes no place closer than 150 m to the one before, but the start that closes it', () => {
		const planner = new RoutePlanner(memory(), ask);
		planner.startAt(WALLBERG);
		// About 100 m east of the Wallberg: one place meant twice.
		planner.add({ lat: WALLBERG.lat, lon: WALLBERG.lon + 0.0013 });
		expect(planner.places).toHaveLength(1);
		// About 200 m east: a place of its own.
		const near = { lat: WALLBERG.lat, lon: WALLBERG.lon + 0.0027 };
		planner.add(near);
		expect(planner.places).toHaveLength(2);
		planner.add(near);
		expect(planner.places).toHaveLength(2);
		// Out east and back to 100 m south of the start: then the start itself
		// closes the route, however close it lies.
		planner.add(EAST);
		planner.add({ lat: WALLBERG.lat - 0.0009, lon: WALLBERG.lon });
		expect(planner.places).toHaveLength(4);
		planner.backToStart();
		expect(planner.places).toHaveLength(5);
		expect(planner.places[4]).toEqual(WALLBERG);
	});

	it('drops an answer for a route that has changed since it was asked', async () => {
		const waiting: ((plan: ReturnType<typeof routePlan>) => void)[] = [];
		const slow = (places: Coordinate[]) =>
			new Promise<ReturnType<typeof routePlan>>((resolve) =>
				waiting.push(() => resolve(routePlan(places)))
			);
		const planner = new RoutePlanner(memory(), slow);
		planner.startAt(WALLBERG);
		planner.add(EAST);
		// The answer for the newest route comes first, the older ones after it.
		waiting.at(-1)?.(routePlan([]));
		await settled();
		for (const answer of waiting.slice(0, -1)) answer(routePlan([]));
		await settled();
		expect(planner.plan?.legs).toHaveLength(1);
	});

	it('says why the core could not be asked', async () => {
		const planner = new RoutePlanner(memory(), () =>
			Promise.reject(new Error('The core stopped.'))
		);
		await settled();
		expect(planner.problem).toBe('The core stopped.');
	});
});
