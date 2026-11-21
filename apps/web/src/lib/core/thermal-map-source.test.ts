import { readFileSync } from 'node:fs';
import { beforeAll, describe, expect, it } from 'vitest';
import { isInWords } from './index';
import { startCore } from './testing';
import { THERMAL_MAP_UNREACHABLE, ThermalMapSource } from './thermal-map-source';
import type { Answer, Get } from './wind-source';

beforeAll(startCore);

/**
 * The server's own recorded answer, which its tests keep current: four
 * squares near the Tegernsee, not flyable on 21 November 2026 and weak
 * thermals on the 22nd.
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
		// Weak thermals are drawn in the scale's second step.
		expect(map.days[1].cells[0]).toMatchObject({
			verdict: 'flyable',
			thermals: 'weak',
			scaleStep: 2,
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
