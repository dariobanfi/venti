import { readFileSync } from 'node:fs';
import { beforeAll, describe, expect, it } from 'vitest';
import { FoehnSource } from './foehn-source';
import { isInWords } from './index';
import { unreachable } from './protocol';
import { startCore } from './testing';
import type { Answer, Get } from './wind-source';

beforeAll(startCore);

/** The server's own recorded answer, which its tests keep current. */
const recorded = (): Uint8Array =>
	readFileSync(new URL('../../../../../crates/server/tests/golden/foehn.json', import.meta.url));

/** The Wallberg's take-off, 75 km from Innsbruck. */
const TEGERNSEE = { lat: 47.6642, lon: 11.7967 };
/** A place far from every pair of stations. */
const HAMBURG = { lat: 53.55, lon: 10.0 };
/** The recorded answer's own moment: 18 November 2026, 19:05 UTC. */
const NOW = Date.UTC(2026, 10, 18, 19, 5) / 1000;

/** A server with the recorded answer, which notes what it was asked. */
function server(answer: () => Promise<Answer> = async () => ({ status: 200, body: recorded() })) {
	const asked: string[] = [];
	const get: Get = (path) => {
		asked.push(path);
		return answer();
	};
	return { asked, get };
}

const source = (get: Get, now = NOW, present = true) =>
	new FoehnSource(
		async () => present,
		async () => now,
		get
	);

describe('foehn, as the worker reads it', () => {
	it('has a line for Today where a pair near the take-offs has something to say', async () => {
		const { asked, get } = server();
		const view = await source(get).read([TEGERNSEE]);
		expect(asked).toEqual(['/v1/foehn']);
		expect(view?.title).toBe('Foehn');
		expect(view?.todayLine).toEqual({
			text: 'South foehn likely: 6 hPa higher at Bolzano than at Innsbruck',
			level: 'likely',
			pairId: 'bolzano-innsbruck'
		});
		expect(view?.rows.map((row) => [row.title, row.level, row.levelLabel])).toEqual([
			['Bolzano – Innsbruck', 'likely', 'Likely'],
			['Klagenfurt – Salzburg', 'none', 'No sign']
		]);
		const [first] = view!.rows;
		expect(first.when).toBe('15 min ago');
		expect(first.points).toHaveLength(12);
		expect(first.aboveLabel).toBe('Higher at Bolzano: south foehn');
		expect(view?.limits.map((limit) => limit.differenceHpa)).toEqual([6, 3, -3, -6]);
		expect(view?.credits.length).toBeGreaterThan(0);
	});

	it('has no line far from every pair, and none without a take-off, but the view all the same', async () => {
		for (const places of [[HAMBURG], []]) {
			const view = await source(server().get).read(places);
			expect(view?.todayLine).toBeUndefined();
			expect(view?.rows).toHaveLength(2);
		}
	});

	it('says the reading’s own time while the server’s clock is not known', async () => {
		const view = await source(server().get, 0).read([TEGERNSEE]);
		expect(view?.todayLine?.text).toBe(
			'South foehn likely at 18:50 UTC: 6 hPa higher at Bolzano than at Innsbruck'
		);
		expect(view?.rows[0].when).toBe('18:50 UTC');
	});

	it('knows nothing of a reading that has grown old', async () => {
		const view = await source(server().get, NOW + 3 * 3600).read([TEGERNSEE]);
		expect(view?.todayLine).toBeUndefined();
		expect(view?.rows[0].level).toBe('unknown');
	});

	it('asks nothing of a site without a server', async () => {
		const { asked, get } = server();
		expect(await source(get, NOW, false).read([TEGERNSEE])).toBeUndefined();
		expect(asked).toEqual([]);
	});

	it('fails in words when the server gives no usable answer', async () => {
		const busy = server(async () => ({ status: 503, body: new Uint8Array() }));
		const refused = await source(busy.get)
			.read([TEGERNSEE])
			.catch((error: unknown) => error);
		expect(isInWords(refused)).toBe(true);
		const down = server(async () => {
			throw unreachable('Venti could not be reached.');
		});
		await expect(source(down.get).read([TEGERNSEE])).rejects.toThrow('Venti could not be reached.');
	});
});
