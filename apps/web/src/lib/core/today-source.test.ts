import { readFileSync } from 'node:fs';
import { beforeAll, describe, expect, it } from 'vitest';
import { startCore } from './testing';
import { nearestTakeoffs, TodaySource } from './today-source';
import type { Answer, Get } from './wind-source';

beforeAll(startCore);

const recorded = (name: string): Uint8Array =>
	readFileSync(new URL(`../../../../../crates/server/tests/golden/${name}`, import.meta.url));

/** Where the development mode's flights take off, beside the Wallberg's ramps. */
const TEGERNSEE = { lat: 47.6642, lon: 11.7967 };
/** The evening of 18 November 2026: the server's recorded forecast is of then. */
const NOW = Date.UTC(2026, 10, 18, 8) / 1000;

/** A server with recorded sites and forecast, the forecast "not yet" for the first `busy` asks. */
function server(busy = 0) {
	const asked: string[] = [];
	const get: Get = async (path): Promise<Answer> => {
		asked.push(path);
		if (path.startsWith('/v1/sites')) return { status: 200, body: recorded('sites.json') };
		if (busy > 0) {
			busy -= 1;
			return { status: 503, body: new Uint8Array() };
		}
		return { status: 200, body: recorded('forecast.json') };
	};
	return { asked, get };
}

describe('today', () => {
	it('lists the take-offs around a place, named ones first, then the nearest', async () => {
		const { get } = server();
		const today = new TodaySource(
			async () => true,
			async () => NOW,
			get,
			0
		);
		const found = (await today.takeoffs(TEGERNSEE))!;
		expect(found.nowUnixS).toBe(NOW);
		expect(found.takeoffs.map((takeoff) => takeoff.site.name)).toEqual([
			'Fluggelände Jägerkamp/Nagelspitz',
			'Take-off',
			'Take-off'
		]);
		expect(found.takeoffs[1].distanceM).toBeLessThan(found.takeoffs[2].distanceM);
	});

	it('asks again for a forecast the server is still fetching, and reads what it says of a take-off', async () => {
		const { asked, get } = server(2);
		const today = new TodaySource(
			async () => true,
			async () => NOW,
			get,
			0
		);
		const { takeoffs } = (await today.takeoffs(TEGERNSEE))!;
		const outlook = await today.outlook(takeoffs[0].site.id, NOW);
		expect(asked.filter((path) => path.startsWith('/v1/forecast'))).toHaveLength(3);
		expect(outlook.days.length).toBeGreaterThan(0);
		expect(['Flyable', 'Marginal', 'Not flyable']).toContain(outlook.days[0].status);
		expect(outlook.note).not.toBe('');
	});

	it('leaves out what is no take-off, is discouraged or lies beyond reach', () => {
		const site = (id: string, lat: number, uses: string[], discouraged = false) =>
			({ id, isNamed: true, uses, discouraged, position: { lat, lon: 11.8 } }) as never;
		const kept = nearestTakeoffs(
			[
				site('landing', 47.66, ['landing']),
				site('discouraged', 47.66, ['takeoff'], true),
				site('far', 48.2, ['takeoff']),
				site('near', 47.67, ['takeoff'])
			],
			TEGERNSEE
		);
		expect(kept.map((takeoff) => takeoff.site.id)).toEqual(['near']);
	});

	it('is not asked for on a site without a server', async () => {
		const { asked, get } = server();
		expect(
			await new TodaySource(
				async () => false,
				async () => NOW,
				get
			).takeoffs(TEGERNSEE)
		).toBe(undefined);
		expect(asked).toEqual([]);
	});
});
