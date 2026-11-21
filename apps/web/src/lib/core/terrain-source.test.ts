import { beforeAll, describe, expect, it } from 'vitest';
import { ONLINE } from '../map/online';
import { flightFromFile, type FlightView } from './pkg/venti_core_wasm.js';
import { type GetTile, PLACES_UNDER_A_FLIGHT, TerrainSource } from './terrain-source';
import { sampleFlight, startCore, tileServer } from './testing';

beforeAll(startCore);

let flight: FlightView;
beforeAll(() => {
	flight = flightFromFile(sampleFlight());
});

const asked = (asking = 1) => ({ asking, bounds: flight.bounds, track: flight.track });

describe('the ground under a flight', () => {
	it('is read from the tiles of the map’s own tile server that cover the flight', async () => {
		const server = tileServer();
		const terrain = new TerrainSource(server.get);

		const ground = await terrain.under(asked());

		// The sample flight lies on two tiles of zoom 12, each asked for
		// once, at the address the map's style has for its relief.
		expect(server.asked.toSorted()).toEqual(
			['12/2181/1429', '12/2182/1429'].map((tile) =>
				ONLINE.terrainTiles.replace('{z}/{x}/{y}', tile)
			)
		);

		// At 240 moments, in order of time, from the first to the last.
		expect(ground).toHaveLength(PLACES_UNDER_A_FLIGHT);
		expect(ground[0].seconds).toBe(0);
		expect(ground.at(-1)!.seconds).toBe(flight.track.at(-1)!.seconds);
		expect(
			ground.every((moment, index) => index === 0 || moment.seconds > ground[index - 1].seconds)
		).toBe(true);

		// The flight starts on the Wallberg, where the tiles give the ground
		// of the take-off as 1621 m, and ends over the valley of the
		// Tegernsee, whose shore lies at some 730 m. It is a made-up flight
		// and not a flown one: for a few moments its line runs through the
		// mountain it started from.
		expect(ground[0].elevationM).toBeCloseTo(1620.7, 0);
		expect(Math.abs(ground[0].heightAboveM)).toBeLessThan(5);
		expect(ground.at(-1)!.elevationM).toBeGreaterThan(725);
		expect(ground.at(-1)!.elevationM).toBeLessThan(740);
		expect(ground.filter((moment) => moment.heightAboveM < 0).length).toBeLessThan(10);
		expect(Math.max(...ground.map((moment) => moment.heightAboveM))).toBeGreaterThan(1000);
		for (const moment of ground) {
			const point = flight.track.find((point) => point.seconds === moment.seconds)!;
			expect(moment.heightAboveM).toBeCloseTo(point.altitudeM - moment.elevationM, 6);
		}
	});

	it('is left out where the tile server has no tile, and empty where it has none at all', async () => {
		// One tile of the flight's is missing at the tile server.
		const whole = tileServer();
		await new TerrainSource(whole.get).under(asked());
		const missing = whole.asked[0];
		const holed = tileServer((address) =>
			address === missing ? Promise.resolve({ status: 404, body: new Uint8Array() }) : undefined
		);
		const ground = await new TerrainSource(holed.get).under(asked());
		expect(holed.asked).toHaveLength(whole.asked.length);
		expect(ground.length).toBeLessThanOrEqual(PLACES_UNDER_A_FLIGHT);

		// No tile anywhere, as over the sea or outside what the server holds.
		const none = tileServer(() => Promise.resolve({ status: 204, body: new Uint8Array() }));
		expect(await new TerrainSource(none.get).under(asked())).toEqual([]);
		expect(none.asked).toHaveLength(whole.asked.length);
	});

	it('is not asked for tile after tile once the tile server gives no answer', async () => {
		const offline = tileServer(() => Promise.reject(new TypeError('Failed to fetch')));

		expect(await new TerrainSource(offline.get).under(asked())).toEqual([]);

		// Those on their way at that moment, and none after them.
		expect(offline.asked.length).toBeLessThanOrEqual(6);
	});

	it('takes bytes that are no elevation tile for a tile it does not have', async () => {
		const page = new TextEncoder().encode('<html>Not here</html>');
		const broken = tileServer(() => Promise.resolve({ status: 200, body: page }));

		expect(await new TerrainSource(broken.get).under(asked())).toEqual([]);

		// Every tile was still asked for: one bad answer is not "no network".
		const whole = tileServer();
		await new TerrainSource(whole.get).under(asked());
		expect(broken.asked).toHaveLength(whole.asked.length);
	});

	it('is worked out for the newest question only, and for none that was let go of', async () => {
		const server = tileServer();
		const terrain = new TerrainSource(server.get);

		// The page opens one flight and then another before the first has
		// its tiles.
		const first = terrain.under(asked(1));
		const second = terrain.under(asked(2));
		expect(await first).toEqual([]);
		expect(await second).toHaveLength(PLACES_UNDER_A_FLIGHT);

		// A flight that is closed has no further tiles fetched for it.
		const before = server.asked.length;
		const third = terrain.under(asked(3));
		terrain.forget(3);
		expect(await third).toEqual([]);
		expect(server.asked.length - before).toBeLessThanOrEqual(6);

		// Letting go of an older question does not stop the newest.
		const fourth = terrain.under(asked(4));
		terrain.forget(3);
		expect(await fourth).toHaveLength(PLACES_UNDER_A_FLIGHT);
	});

	it('reads the ground under a flight of eight hours without holding the worker up for long', async () => {
		// 28,800 positions, one a second, back and forth over the sample
		// flight's track: what crosses to the core is the whole track.
		const track = Array.from({ length: 28_800 }, (_, seconds) => ({
			...flight.track[seconds % flight.track.length],
			seconds
		}));
		const terrain = new TerrainSource(tileServer().get);

		const began = performance.now();
		const ground = await terrain.under({ asking: 1, bounds: flight.bounds, track });
		const took = performance.now() - began;

		expect(ground).toHaveLength(PLACES_UNDER_A_FLIGHT);
		// Decoding the tiles is most of it. Generous: test machines are slow.
		expect(took).toBeLessThan(5000);
	});
});
