import { beforeAll, describe, expect, it } from 'vitest';
import { cellMiddles, SlopesSource } from './slopes-source';
import { startCore, tileServer } from './testing';

beforeAll(startCore);

/** Around the Wallberg, inside the valley the test tiles hold: some 3 km by 2 km. */
const WALLBERG = { south: 47.655, west: 11.775, north: 47.675, east: 11.815 };

describe('the map of slopes', () => {
	it('places a grid’s cells at their middles, row after row from the north-west', () => {
		const middles = cellMiddles({ south: 0, west: 0, north: 2, east: 4 }, 2, 2);
		expect(middles).toEqual([
			{ lat: 1.5, lon: 1 },
			{ lat: 1.5, lon: 3 },
			{ lat: 0.5, lon: 1 },
			{ lat: 0.5, lon: 3 }
		]);
	});

	it('classes every cell of the box over the tiles fetched for it, steep and gentle', async () => {
		const server = tileServer();
		const slopes = new SlopesSource(server.get);
		const answer = await slopes.classes({ asking: 1, box: WALLBERG, columns: 60, rows: 40 });

		expect(answer?.classes).toHaveLength(2400);
		// Every cell has ground, and the Wallberg has both the valley's
		// gentle floor and slopes of 35° and more.
		expect(answer!.classes.every((one) => one >= 1 && one <= 5)).toBe(true);
		expect(answer!.classes.filter((one) => one === 1).length).toBeGreaterThan(100);
		expect(answer!.classes.filter((one) => one >= 4).length).toBeGreaterThan(10);

		// The tiles are kept: the same box again fetches nothing.
		const before = server.asked.length;
		await slopes.classes({ asking: 2, box: WALLBERG, columns: 60, rows: 40 });
		expect(server.asked).toHaveLength(before);
		slopes.forget();
	});

	it('leaves a cell without a class where the tile server has no ground', async () => {
		const slopes = new SlopesSource(async () => ({ status: 404, body: new Uint8Array() }));
		const answer = await slopes.classes({ asking: 1, box: WALLBERG, columns: 4, rows: 3 });
		expect([...answer!.classes]).toEqual(Array(12).fill(0));
		slopes.forget();
	});

	it('answers only the newest question', async () => {
		const server = tileServer();
		const slopes = new SlopesSource(server.get);
		const first = slopes.classes({ asking: 1, box: WALLBERG, columns: 4, rows: 3 });
		const second = slopes.classes({ asking: 2, box: WALLBERG, columns: 4, rows: 3 });
		expect(await first).toBeUndefined();
		expect((await second)?.asking).toBe(2);
		slopes.forget();
	});
});
