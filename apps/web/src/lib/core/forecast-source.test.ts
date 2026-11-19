import { readFileSync } from 'node:fs';
import { beforeAll, describe, expect, it } from 'vitest';
import { ForecastSource } from './forecast-source';
import { startCore } from './testing';
import type { Answer, Get } from './wind-source';

beforeAll(startCore);

/**
 * The server's own recorded answer, which its tests keep current: one hour
 * of ICON-D2 and one of ICON-EU for the east shore of the Tegernsee.
 */
const recorded = (): Uint8Array =>
	readFileSync(new URL('../../../../../crates/server/tests/golden/forecast.json', import.meta.url));

const TEGERNSEE = { lat: 47.7, lon: 11.8 };

/** Central European Time, an hour ahead of UTC. */
const CET = () => 3600;

/** A server that gives one answer and notes what it was asked. */
function server(answer: () => Answer = () => ({ status: 200, body: recorded() })) {
	const asked: string[] = [];
	const get: Get = async (path) => {
		asked.push(path);
		return answer();
	};
	return { asked, get };
}

const refusal = (status: number, code: string): Answer => ({
	status,
	body: new TextEncoder().encode(JSON.stringify({ error: { code, message: 'for developers' } }))
});

describe('the forecast for a place, from the server', () => {
	it('is asked for by the place, and read model by model, the finest first', async () => {
		const { asked, get } = server();
		const forecast = (await new ForecastSource(async () => true, get, CET).forecast(TEGERNSEE))!;

		expect(asked).toEqual(['/v1/forecast?lat=47.7&lon=11.8']);
		expect(forecast.position).toEqual(TEGERNSEE);
		expect(forecast.models.map((model) => [model.name, model.grid])).toEqual([
			['ICON-D2', '2 km cells'],
			['ICON-EU', '7 km cells']
		]);
		const [d2] = forecast.models;
		// Each model with its run, and how high it thinks the ground is.
		expect(d2.runUnixS).toBe(Date.UTC(2026, 10, 19, 18) / 1000);
		expect(d2.cellElevationM).toBe(1069);
		// The heights above ground first, then the pressure surfaces, lowest first.
		expect(d2.levels.map((level) => level.name)).toEqual([
			'80 m above ground',
			'120 m above ground',
			'180 m above ground',
			'850 hPa',
			'700 hPa',
			'600 hPa',
			'500 hPa'
		]);
		expect(d2.levels[3].aboutAltitudeM).toBe(1300);
	});

	it('gives every wind with its strength as the map draws it, and its speeds as a label', async () => {
		const { get } = server();
		const forecast = (await new ForecastSource(async () => true, get, CET).forecast(TEGERNSEE))!;

		const hour = forecast.models[0].days[0].hours[0];
		// 2.4 m/s from 225° with gusts of 13.1: light, written in km/h.
		expect(hour.wind).toEqual({ speedMs: 2.4, fromDeg: 225, band: 'light', label: '9/47' });
		expect(hour.aloft).toHaveLength(7);
		expect(hour.aloft[4]).toMatchObject({ band: 'strong', fromDeg: 277 });
		// A number, not a BigInt.
		expect(hour.timeUnixS).toBe(Date.UTC(2026, 10, 19, 19) / 1000);
		expect(hour.isDaylight).toBe(false);
	});

	it('cuts the days by the clock it is given', async () => {
		const { get } = server();
		// 19:00 UTC is 20:00 in Munich and 04:00 the next day in Tokyo.
		const here = (await new ForecastSource(async () => true, get, CET).forecast(TEGERNSEE))!;
		const tokyo = (await new ForecastSource(
			async () => true,
			get,
			() => 9 * 3600
		).forecast(TEGERNSEE))!;

		expect(here.models[0].days[0].startUnixS).toBe(Date.UTC(2026, 10, 18, 23) / 1000);
		expect(tokyo.models[0].days[0].startUnixS).toBe(Date.UTC(2026, 10, 19, 15) / 1000);
	});

	it('comes with the credit of its source', async () => {
		const { get } = server();
		const forecast = (await new ForecastSource(async () => true, get, CET).forecast(TEGERNSEE))!;

		expect(forecast.credits).toEqual([
			expect.objectContaining({
				source: 'open-meteo',
				text: expect.stringContaining('Open-Meteo.com'),
				url: 'https://open-meteo.com/'
			})
		]);
	});

	it('is not asked for on a site without a server', async () => {
		const { asked, get } = server();

		expect(
			await new ForecastSource(async () => false, get, CET).forecast(TEGERNSEE)
		).toBeUndefined();
		expect(asked).toEqual([]);
	});

	it('says in the core’s words why there is none', async () => {
		const none = new ForecastSource(
			async () => true,
			server(() => refusal(501, 'no_forecasts')).get,
			CET
		);
		const outside = new ForecastSource(
			async () => true,
			server(() => refusal(404, 'outside_region')).get,
			CET
		);
		const busy = new ForecastSource(
			async () => true,
			server(() => refusal(503, 'unavailable')).get,
			CET
		);

		await expect(none.forecast(TEGERNSEE)).rejects.toMatchObject({
			name: 'Server',
			message: 'This server has no forecasts.'
		});
		await expect(outside.forecast(TEGERNSEE)).rejects.toThrow(
			'This place lies outside the area the server has forecasts for.'
		);
		await expect(busy.forecast(TEGERNSEE)).rejects.toThrow(
			'The server is busy or being restarted. Try again in a moment.'
		);
	});
});
