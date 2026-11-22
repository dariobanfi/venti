import { readFileSync } from 'node:fs';
import { beforeAll, describe, expect, it } from 'vitest';
import { isInWords } from './index';
import { startCore } from './testing';
import { THERMAL_MAP_UNREACHABLE, ThermalMapSource } from './thermal-map-source';
import type { Answer, Get } from './wind-source';

beforeAll(startCore);

/**
 * The server's own recorded answer, which its tests keep current: four
 * squares near the Tegernsee, not flyable on 21 November 2026 and with
 * nothing to climb in on the 22nd.
 */
const recorded = (): Uint8Array =>
	readFileSync(new URL('../../../../../crates/server/tests/golden/thermals.json', import.meta.url));

/** Noon on 21 November 2026, UTC. */
const NOON_21 = Date.UTC(2026, 10, 21, 12) / 1000;

function server(answer: () => Answer = () => ({ status: 200, body: recorded() })) {
	const asked: string[] = [];
	const get: Get = async (path) => {
		asked.push(path);
		return answer();
	};
	return { asked, get };
}

describe('the thermal map from the server', () => {
	it('is asked for whole, and read by the core from today on', async () => {
		const { asked, get } = server();
		const source = new ThermalMapSource(
			async () => true,
			async () => NOON_21,
			get
		);

		const map = (await source.map())!;

		expect(asked).toEqual(['/v1/thermals']);
		expect(map.model).toBe('ICON-D2');
		expect(map.days.map((day) => [day.date, day.name])).toEqual([
			['2026-11-21', 'Today'],
			['2026-11-22', 'Tomorrow'],
			['2026-11-23', 'Mon'],
			['2026-11-24', 'Tue']
		]);
		// A day that is not flyable has no kind of day: the veil.
		expect(map.days[0].cells).toHaveLength(4);
		expect(map.days[0].cells[0]).toMatchObject({
			verdict: 'notFlyable',
			scaleStep: undefined,
			status: 'Not flyable',
			thermalWords: ''
		});
		// A flyable day without thermals is drawn in the scale's first step.
		expect(map.days[1].cells[0]).toMatchObject({
			verdict: 'flyable',
			thermals: 'noThermals',
			scaleStep: 1,
			groundM: 1118
		});
		expect(map.legend.at(-1)).toEqual({ words: 'Not flyable', scaleStep: undefined });
		expect(map.credits.map((credit) => credit.source)).toEqual(['open-meteo']);
	});

	it('is not asked for where the site has no server', async () => {
		const { asked, get } = server();
		const source = new ThermalMapSource(
			async () => false,
			async () => NOON_21,
			get
		);
		expect(await source.map()).toBeUndefined();
		expect(asked).toEqual([]);
	});

	it('says in the core’s words when the server has no map', async () => {
		const { get } = server(() => ({ status: 503, body: new Uint8Array() }));
		const source = new ThermalMapSource(
			async () => true,
			async () => NOON_21,
			get
		);
		const failure = await source.map().catch((error: unknown) => error);
		expect(isInWords(failure)).toBe(true);
		expect((failure as Error).message).not.toBe('');
	});

	it('has words of its own for a server that does not answer', () => {
		expect(THERMAL_MAP_UNREACHABLE).toBe('Venti could not be reached for the forecast map.');
	});
});

/** The server's recorded hours of the 22nd: thermals, and the wind at 1000 m. */
const recordedFile = (name: string): Uint8Array =>
	readFileSync(new URL(`../../../../../crates/server/tests/golden/${name}`, import.meta.url));

/** A server with the map of the days and the hours it recorded. */
function hoursServer() {
	const asked: string[] = [];
	const get: Get = async (path) => {
		asked.push(path);
		if (path === '/v1/thermals') return { status: 200, body: recorded() };
		if (path.includes('layer=wind')) {
			return { status: 200, body: recordedFile('thermal_hours_wind.json') };
		}
		return { status: 200, body: recordedFile('thermal_hours.json') };
	};
	return { asked, get };
}

describe('the thermal map hour by hour', () => {
	it('reads a day’s hours once, and draws each hour from what it kept', async () => {
		const { asked, get } = hoursServer();
		const source = new ThermalMapSource(
			async () => true,
			async () => NOON_21,
			get
		);
		const map = (await source.map())!;
		expect(map.layers.map((layer) => layer.layer)).toEqual([
			'thermals',
			'wind',
			'gusts',
			'cloud',
			'rain'
		]);
		expect(map.levels[0].name).toBe('Ground');

		const asking = { date: '2026-11-22', layer: 'thermals' } as const;
		const view = await source.hours(asking);
		await source.hours(asking);
		expect(asked.filter((path) => path.startsWith('/v1/thermals/hours'))).toHaveLength(1);
		expect(view.hours.length).toBeGreaterThan(0);
		expect(view.hours[0].label).toMatch(/^\d\d:00$/);

		const shapes = source.shapes(asking, view.firstHour);
		const squares = JSON.parse(shapes.squares) as GeoJSON.FeatureCollection;
		expect(squares.type).toBe('FeatureCollection');
		expect(JSON.parse(shapes.arrows).features).toEqual([]);

		// A place on the grid has a square, whether one is drawn there or not.
		const cell = source.cellAt(asking, 47.7, 11.8);
		expect(cell).toBeTypeOf('number');
		expect(source.square(asking, cell!, view.firstHour)?.title).not.toBe('');
	});

	it('reads the wind at a height, and lets go of the hours when another map comes', async () => {
		const { get } = hoursServer();
		const source = new ThermalMapSource(
			async () => true,
			async () => NOON_21,
			get
		);
		await source.map();
		const asking = { date: '2026-11-22', layer: 'wind', heightM: 1000 } as const;
		const view = await source.hours(asking);
		expect(view.heightM).toBe(1000);
		expect(view.layer).toBe('wind');

		const strip = await source.strip(47.7, 11.8);
		expect(strip.map((day) => day.date)).toContain('2026-11-22');
	});

	it('says in the core’s words when the server refuses the hours, and asks again later', async () => {
		let refusing = true;
		const asked: string[] = [];
		const get: Get = async (path) => {
			asked.push(path);
			if (path === '/v1/thermals') return { status: 200, body: recorded() };
			return refusing
				? { status: 503, body: new Uint8Array() }
				: { status: 200, body: recordedFile('thermal_hours.json') };
		};
		const source = new ThermalMapSource(
			async () => true,
			async () => NOON_21,
			get
		);
		await source.map();
		const asking = { date: '2026-11-22', layer: 'thermals' } as const;
		const failure = await source.hours(asking).catch((error: unknown) => error);
		expect(isInWords(failure)).toBe(true);
		refusing = false;
		expect((await source.hours(asking)).hours.length).toBeGreaterThan(0);
	});
});
