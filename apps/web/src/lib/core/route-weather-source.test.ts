import { readFileSync } from 'node:fs';
import { beforeAll, describe, expect, it } from 'vitest';
import { RouteWeatherSource } from './route-weather-source';
import { startCore } from './testing';

beforeAll(startCore);

/** The development mode's answer for the Wallberg, a place south-east of it and the Brauneck, on 22 November 2026. */
const recorded = readFileSync(
	new URL('../../../../../crates/server/tests/golden/route_weather.json', import.meta.url)
);

const places = [
	{ lat: 47.6642, lon: 11.7967, name: 'Wallberg' },
	{ lat: 47.62, lon: 11.91 },
	{ lat: 47.6603, lon: 11.5197, name: 'Brauneck' }
];
/** 10:00 in Berlin on 22 November 2026. */
const START = Date.UTC(2026, 10, 22, 9) / 1000;
const setting = { places, startUnixS: START, groundSpeedKmh: 20, glideRatio: 8 };
const BERLIN = () => 3600;
const noTiles = async () => ({ status: 404, body: new Uint8Array() });

describe('the weather along a route', () => {
	it('asks the server once for a route and a day, and the core again for every setting', async () => {
		const asked: string[] = [];
		const source = new RouteWeatherSource(
			async () => true,
			async (path) => {
				asked.push(path);
				return { status: 200, body: new Uint8Array(recorded) };
			},
			noTiles,
			'https://tiles.example/{z}/{x}/{y}.webp',
			BERLIN
		);
		const { along, problem } = await source.along({ setting, date: '2026-11-22' });
		expect(problem).toBeUndefined();
		expect(asked).toEqual([
			'/v1/route/weather?date=2026-11-22&points=47.6642,11.7967;47.62,11.91;47.6603,11.5197'
		]);
		expect(along.points).toHaveLength(3);
		expect(along.legs).toHaveLength(2);
		expect(along.points[0].clock).toBe('10:00');
		expect(along.points[0].words).toMatch(/^Wallberg at 10:00/);
		expect(along.model).toBeTruthy();
		expect(along.words).toContain('20 km/h');

		// Faster: the same answer, read again by the core.
		const faster = await source.along({
			setting: { ...setting, groundSpeedKmh: 30 },
			date: '2026-11-22'
		});
		expect(asked).toHaveLength(1);
		expect(faster.along.finishUnixS).toBeLessThan(along.finishUnixS);
	});

	it('says the times without a day, and why there is no forecast when the server has none', async () => {
		const source = new RouteWeatherSource(
			async () => true,
			async () => ({ status: 503, body: new Uint8Array() }),
			noTiles,
			'https://tiles.example/{z}/{x}/{y}.webp',
			BERLIN
		);
		const without = await source.along({ setting });
		expect(without.problem).toBeUndefined();
		expect(without.along.model).toBeUndefined();
		expect(without.along.points[1].clock).toMatch(/^\d\d:\d\d$/);

		const refused = await source.along({ setting, date: '2026-11-22' });
		expect(refused.problem).toBeTruthy();
		expect(refused.along.legs).toHaveLength(2);
	});
});
