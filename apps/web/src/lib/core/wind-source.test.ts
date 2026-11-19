import { readFileSync } from 'node:fs';
import { beforeAll, describe, expect, it } from 'vitest';
import { startCore } from './testing';
import { type Answer, type Get, WindSource } from './wind-source';

beforeAll(startCore);

/**
 * The server's own recorded answers, which its tests keep current: six
 * stations as they were on 18 November 2026 at 19:05 UTC.
 */
function recorded(name: string): Uint8Array {
	return readFileSync(
		new URL(`../../../../../crates/server/tests/golden/${name}`, import.meta.url)
	);
}

/** 18 November 2026, 19:05 UTC, in milliseconds. */
const SERVER_NOW = 1_795_028_700_000;

const ALPS = { south: 46.5, west: 10, north: 48.5, east: 13.5 };

/** A server that answers from the recorded files and notes what it was asked. */
function server(answers: Record<string, Answer> = {}) {
	const asked: string[] = [];
	const get: Get = async (path) => {
		asked.push(path);
		const name = path.split('?')[0];
		if (answers[name]) return answers[name];
		if (name === '/v1/health') return { status: 200, body: recorded('health.json') };
		if (name === '/v1/stations') return { status: 200, body: recorded('stations.json') };
		return { status: 200, body: recorded('observations.json') };
	};
	return { asked, get };
}

/** A clock this device believes in, to be set by the test. */
function device(startsAt: number) {
	const clock = { now: startsAt };
	return { clock, read: () => clock.now };
}

describe('wind from the server', () => {
	it('reads the stations of a box, with the credit of every source', async () => {
		const { asked, get } = server();
		const wind = new WindSource(get, () => SERVER_NOW);

		const answer = await wind.windMap(ALPS);

		expect(asked).toEqual(['/v1/health', '/v1/stations?bbox=10,46.5,13.5,48.5']);
		expect(answer?.map.stations.map((station) => station.name)).toContain('Zugspitze');
		expect(answer?.map.credits.map((credit) => credit.provider)).toEqual([
			'Deutscher Wetterdienst',
			'GeoSphere Austria'
		]);
		expect(answer?.lateProviders).toEqual([]);
	});

	it('measures the age of a reading against the server’s clock, not the device’s', async () => {
		// This device's clock is three hours fast.
		const { get } = server();
		const wind = new WindSource(get, () => SERVER_NOW + 3 * 3600_000);

		const answer = await wind.windMap(ALPS);

		const zugspitze = answer!.map.stations.find((station) => station.name === 'Zugspitze')!;
		// Measured at 18:20, and it is 19:05 on the server.
		expect(zugspitze.ageMin).toBe(45);
		expect(zugspitze.isOld).toBe(false);
	});

	it('hands over how the core wants each station drawn', async () => {
		const { get } = server();
		const wind = new WindSource(get, () => SERVER_NOW);

		const stations = (await wind.windMap(ALPS))!.map.stations;

		// 14.9 m/s from 260° with gusts of 25.7: a strong wind blowing to 80°.
		const zugspitze = stations.find((station) => station.name === 'Zugspitze')!;
		expect(zugspitze.mark).toEqual({
			symbol: { kind: 'arrow', band: 'strong' },
			headingDeg: 80,
			label: '54/93'
		});
		// Every mark is one of the three the app has a picture for.
		for (const station of stations) {
			expect(['arrow', 'calm', 'old']).toContain(station.mark.symbol.kind);
		}
	});

	it('lets time pass on the server’s clock between two looks at it', async () => {
		const { asked, get } = server();
		const { clock, read } = device(SERVER_NOW - 5000);
		const wind = new WindSource(get, read);
		await wind.windMap(ALPS);

		// Half a minute later the clock is not read again, but has moved on.
		clock.now += 30_000;
		await wind.windMap(ALPS);
		expect(asked.filter((path) => path === '/v1/health')).toHaveLength(1);

		// After a minute it is.
		clock.now += 31_000;
		await wind.windMap(ALPS);
		expect(asked.filter((path) => path === '/v1/health')).toHaveLength(2);
	});

	it('names the providers the server has not heard from', async () => {
		const late = JSON.parse(new TextDecoder().decode(recorded('health.json')));
		late.sources[0].stale = true;
		const { get } = server({
			'/v1/health': { status: 200, body: new TextEncoder().encode(JSON.stringify(late)) }
		});

		const answer = await new WindSource(get, () => SERVER_NOW).windMap(ALPS);

		expect(answer?.lateProviders).toEqual(['Deutscher Wetterdienst']);
	});

	it('says who may create an account, by the server’s own word', async () => {
		/** The recorded health answer with another word about registration, or none. */
		const saying = (registration: string | undefined) => {
			const health = JSON.parse(new TextDecoder().decode(recorded('health.json')));
			if (registration === undefined) delete health.registration;
			else health.registration = registration;
			const { get } = server({
				'/v1/health': { status: 200, body: new TextEncoder().encode(JSON.stringify(health)) }
			});
			return new WindSource(get, () => SERVER_NOW).registration();
		};

		// The development mode, as recorded, takes anyone.
		expect(await new WindSource(server().get, () => SERVER_NOW).registration()).toBe('open');
		expect(await saying('invite')).toBe('invite');
		expect(await saying('closed')).toBe('closed');
		// A server older than the question, and a rule this app does not
		// know: no answer, and a form then offers the field for a code.
		expect(await saying(undefined)).toBeUndefined();
		expect(await saying('by_email')).toBeUndefined();
	});

	it('has no word on registration from a site without a server, or one out of reach', async () => {
		const none = server({
			'/v1/health': { status: 404, body: new TextEncoder().encode('<h1>Not found</h1>') }
		});
		const unreachable: typeof none.get = async () => {
			throw new Error('Failed to fetch');
		};

		expect(await new WindSource(none.get, () => SERVER_NOW).registration()).toBeUndefined();
		expect(await new WindSource(unreachable, () => SERVER_NOW).registration()).toBeUndefined();
	});

	it('has no wind, and asks no more, on a site without a server', async () => {
		const { asked, get } = server({
			'/v1/health': { status: 404, body: new TextEncoder().encode('<h1>Not found</h1>') }
		});
		const wind = new WindSource(get, () => SERVER_NOW);

		expect(await wind.windMap(ALPS)).toBeUndefined();
		expect(await wind.windMap(ALPS)).toBeUndefined();

		expect(asked).toEqual(['/v1/health']);
	});

	it('puts a server in trouble in the core’s words', async () => {
		const { get } = server({ '/v1/health': { status: 502, body: new Uint8Array() } });

		const failure = await new WindSource(get, () => SERVER_NOW)
			.windMap(ALPS)
			.catch((error: Error) => error);

		expect((failure as Error).name).toBe('Server');
		expect((failure as Error).message).toBe('The server could not answer (status 502).');
	});

	it('takes a server that answered before and now says “not found” to be in trouble', async () => {
		const answers: Record<string, Answer> = {};
		const { get } = server(answers);
		const { clock, read } = device(SERVER_NOW);
		const wind = new WindSource(get, read);
		await wind.windMap(ALPS);

		answers['/v1/health'] = { status: 404, body: new Uint8Array() };
		clock.now += 61_000;
		const failure = await wind.windMap(ALPS).catch((error: Error) => error);

		expect((failure as Error).name).toBe('Server');
	});

	it('reads a station’s last 24 hours', async () => {
		const { asked, get } = server();
		const wind = new WindSource(get, () => SERVER_NOW);
		await wind.windMap(ALPS);

		const history = await wind.stationHistory('dwd-10min:05792');

		expect(asked.at(-1)).toBe('/v1/stations/dwd-10min:05792/observations');
		expect(history.readings.length).toBeGreaterThan(1);
		expect(history.station.latest).toEqual(history.readings.at(-1));
		expect(history.credits).toHaveLength(1);
	});

	it('refuses a station identifier that is none, without asking the server', async () => {
		const { asked, get } = server();

		const failure = await new WindSource(get, () => SERVER_NOW)
			.stationHistory('not an id')
			.catch((error: Error) => error);

		expect((failure as Error).name).toBe('Server');
		expect(asked).toEqual([]);
	});
});
