import { beforeAll, describe, expect, it } from 'vitest';
import { boxAround, GlideReachSource } from './glide-reach-source';
import { startCore, tileServer } from './testing';

beforeAll(startCore);

/** The Wallberg's summit, inside the region whose tiles the repository keeps. */
const WALLBERG = { lat: 47.6642, lon: 11.7967, glideRatio: 8 };
const TILES = 'https://tiles.example/{z}/{x}/{y}.webp';

describe('what a glide reaches from a place', () => {
	it('fetches the ground around the place and asks the core, in its words', async () => {
		const server = tileServer();
		const source = new GlideReachSource(server.get, TILES);

		const reach = await source.reach(WALLBERG);

		expect(server.asked.length).toBeGreaterThan(0);
		expect(server.asked.every((address) => address.startsWith('https://tiles.example/'))).toBe(
			true
		);
		expect(reach.words).toMatch(/^From \d+ m at a glide of 8/);
		expect(reach.outline.length).toBeGreaterThan(3);
		expect(reach.outline[0]).toEqual(reach.outline.at(-1));
		expect(reach.groundM).toBeGreaterThan(1500);
		expect(reach.note).toBeTruthy();
		source.forget();
	});

	it('asks the core again for another setting at the same place, and fetches nothing', async () => {
		const server = tileServer();
		const source = new GlideReachSource(server.get, TILES);
		const still = await source.reach(WALLBERG);
		expect(still.cutShort).toBe(false);
		const fetched = server.asked.length;

		const better = await source.reach({ ...WALLBERG, glideRatio: 9.5 });
		const windy = await source.reach({ ...WALLBERG, windFromDeg: 270, windKmh: 20 });

		expect(server.asked).toHaveLength(fetched);
		expect(better.glideRatio).toBe(9.5);
		expect(better.farthestM).toBeGreaterThan(still.farthestM);
		expect(windy.words).not.toBe(still.words);
		source.forget();
	});

	it('fetches the tiles of a wider box while the glide runs off the ground, up to the widest', async () => {
		const server = tileServer();
		const source = new GlideReachSource(server.get, TILES);
		await source.reach(WALLBERG);
		const fetched = server.asked.length;

		const high = await source.reach({ ...WALLBERG, startHeightM: 4000 });

		// The repository's region ends within the widest box: the glide
		// still runs off it, and the core says so.
		expect(server.asked.length).toBeGreaterThan(fetched);
		expect(high.cutShort).toBe(true);
		expect(high.words).toContain('Ground not loaded');
		expect(high.startM).toBe(4000);
		source.forget();
	});

	it('draws no area without the ground, as when the tile server cannot be reached', async () => {
		const offline = tileServer(() => Promise.reject(new TypeError('Failed to fetch')));
		const source = new GlideReachSource(offline.get, TILES);

		const reach = await source.reach(WALLBERG);

		expect(reach.outline).toEqual([]);
		expect(reach.words).toBeTruthy();
		source.forget();
	});

	it('lets an answer for a place asked about since go, with no area', async () => {
		const server = tileServer();
		const source = new GlideReachSource(server.get, TILES);
		const first = source.reach(WALLBERG);
		const second = source.reach({ ...WALLBERG, lat: 47.67, lon: 11.78 });

		expect((await first).outline).toEqual([]);
		expect((await second).words).toMatch(/^From /);
		source.forget();
	});
});

describe('the box around a place', () => {
	it('reaches as far each way, in kilometres', () => {
		const box = boxAround(47.6642, 11.7967, 10);
		expect((box.north - box.south) * 111.32).toBeCloseTo(20, 5);
		expect((box.east - box.west) * 111.32 * Math.cos((47.6642 * Math.PI) / 180)).toBeCloseTo(20, 5);
	});
});
