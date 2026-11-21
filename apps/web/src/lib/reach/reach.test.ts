import { describe, expect, it } from 'vitest';
import type { GlideReach, GlideReachSetting, ReachRay } from '#lib/core/index.ts';
import { reachArea, reachEdge, reachStart } from './on-map';
import { GlideReachModel } from './reach.svelte';

const WALLBERG = { lat: 47.6642, lon: 11.7967 };
const settled = () => new Promise((resolve) => setTimeout(resolve, 0));

function memory(kept: Record<string, string> = {}) {
	return { getItem: (key: string) => kept[key] ?? null, setItem: () => {} };
}

const ray = (bearingDeg: number, end: ReachRay['end']): ReachRay => ({
	bearingDeg,
	distanceM: 1000,
	endAt: { lat: 47 + bearingDeg / 1000, lon: 11 },
	arrivalM: 1000,
	overGroundM: end === 'notLoaded' ? undefined : 200,
	end
});

const answer = (setting: GlideReachSetting, rays: ReachRay[] = []): GlideReach =>
	({
		outline: [...rays.map((one) => one.endAt), ...rays.slice(0, 1).map((one) => one.endAt)],
		rays,
		words: `From ${setting.startHeightM ?? 'the ground'} at a glide of ${setting.glideRatio}`,
		note: 'What it does not know.',
		cutShort: false
	}) as unknown as GlideReach;

describe('what a glide reaches, as the map tab asks for it', () => {
	it('starts with the glide the route planner keeps, from the ground, in calm air', async () => {
		const asked: GlideReachSetting[] = [];
		const model = new GlideReachModel(
			memory({ 'venti.route-setting': JSON.stringify({ speedKmh: 20, glide: 9.5 }) }),
			async (setting) => (asked.push(setting), answer(setting))
		);
		await model.open(WALLBERG);
		expect(asked).toEqual([
			{
				...WALLBERG,
				glideRatio: 9.5,
				startHeightM: undefined,
				windFromDeg: undefined,
				windKmh: undefined,
				marginM: undefined
			}
		]);
		expect(model.reach?.words).toBe('From the ground at a glide of 9.5');
		expect(model.asking).toBe(false);
	});

	it('is 8 without a glide kept, and asks again for each setting with the wind once it is on', async () => {
		const asked: GlideReachSetting[] = [];
		const model = new GlideReachModel(
			memory(),
			async (setting) => (asked.push(setting), answer(setting))
		);
		await model.open(WALLBERG);
		expect(asked[0].glideRatio).toBe(8);

		model.startHeightM = 2000;
		await model.show();
		model.windOn = true;
		await model.show();
		expect(asked.at(-1)).toMatchObject({ startHeightM: 2000, windFromDeg: 270, windKmh: 15 });
		expect(model.reach?.words).toBe('From 2000 at a glide of 8');
	});

	it('drops an answer for a setting changed since', async () => {
		const waiting: ((reach: GlideReach) => void)[] = [];
		const model = new GlideReachModel(
			memory(),
			(setting) => new Promise((resolve) => waiting.push(() => resolve(answer(setting))))
		);
		void model.open(WALLBERG);
		model.glide = 10;
		void model.show();
		waiting[1](answer({ ...WALLBERG, glideRatio: 10 }));
		await settled();
		waiting[0](answer({ ...WALLBERG, glideRatio: 8 }));
		await settled();
		expect(model.reach?.words).toBe('From the ground at a glide of 10');
	});

	it('clears the area, lets go of the ground, and drops an answer still coming', async () => {
		let forgotten = 0;
		let resolve: (reach: GlideReach) => void = () => {};
		const model = new GlideReachModel(
			memory(),
			(setting) => new Promise((done) => (resolve = () => done(answer(setting)))),
			() => (forgotten += 1)
		);
		void model.open(WALLBERG);
		model.clear();
		resolve(answer({ ...WALLBERG, glideRatio: 8 }));
		await settled();
		expect(model.place).toBeUndefined();
		expect(model.reach).toBeUndefined();
		expect(forgotten).toBe(1);
	});

	it('says why where the core could not answer', async () => {
		const model = new GlideReachModel(memory(), async () => {
			throw new Error('The core could not start.');
		});
		await model.open(WALLBERG);
		expect(model.problem).toBe('The core could not start.');
	});
});

describe('what the map draws of a glide’s reach', () => {
	it('fills the core’s ring and marks the start', () => {
		const rays = [ray(0, 'ground'), ray(120, 'ground'), ray(240, 'ground')];
		const area = reachArea(answer({ ...WALLBERG, glideRatio: 8 }, rays));
		expect(area.features).toHaveLength(1);
		expect(area.features[0].geometry).toMatchObject({ type: 'Polygon' });
		expect(reachArea(undefined).features).toEqual([]);
		expect(reachStart(WALLBERG).features[0].geometry).toEqual({
			type: 'Point',
			coordinates: [WALLBERG.lon, WALLBERG.lat]
		});
		expect(reachStart(undefined).features).toEqual([]);
	});

	it('dashes the edge where the ground loaded ends, in runs all round', () => {
		const rays = [ray(0, 'ground'), ray(90, 'ground'), ray(180, 'notLoaded'), ray(270, 'ground')];
		const edge = reachEdge(answer({ ...WALLBERG, glideRatio: 8 }, rays));
		expect(edge.features.map((feature) => feature.properties?.dashed)).toEqual([
			false,
			true,
			false
		]);
		// From 0 to 90 solid; 90 to 180 and 180 to 270 dashed; 270 back to 0 solid.
		expect(
			edge.features.map((feature) => (feature.geometry as GeoJSON.LineString).coordinates.length)
		).toEqual([2, 3, 2]);
		expect(reachEdge(undefined).features).toEqual([]);
	});
});
