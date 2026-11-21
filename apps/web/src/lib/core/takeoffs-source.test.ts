import { readFileSync } from 'node:fs';
import { beforeAll, describe, expect, it } from 'vitest';
import { CoreSession } from './session';
import { MemoryStore } from './store';
import { TakeoffsSource } from './takeoffs-source';
import { startCore } from './testing';
import type { Get } from './wind-source';

beforeAll(startCore);

/** The server's own recorded answers, which its tests keep current. */
function recorded(name: string): Uint8Array {
	return readFileSync(
		new URL(`../../../../../crates/server/tests/golden/${name}`, import.meta.url)
	);
}

/** The development mode's made-up take-off. */
const KOGEL = 'venti:0000000000000000000000s001';

/** A server played from the recorded answers, noting what was asked of it. */
function server(asked: string[]): Get {
	return async (path) => {
		asked.push(path);
		if (path.startsWith('/v1/sites/candidates')) {
			return { status: 200, body: recorded('site_candidates.json') };
		}
		if (path.endsWith('/flights')) return { status: 200, body: recorded('site_flights.json') };
		return { status: 404, body: new Uint8Array() };
	};
}

describe('what public flights say about where pilots take off', () => {
	it("reads a site's flights in the core's words, asked by the core's path", async () => {
		const asked: string[] = [];
		const session = await CoreSession.open(new MemoryStore());
		const source = new TakeoffsSource(session, async () => true, server(asked));
		const flights = await source.site(KOGEL);
		expect(asked).toHaveLength(1);
		expect(asked[0]).toMatch(/^\/v1\/sites\/.+\/flights$/);
		expect(flights.isTakeoff).toBe(true);
		expect(flights.words).toMatch(/public flights by \d+ pilots began here/);
		expect(flights.months).toHaveLength(12);
		expect(flights.months[0].name).toBe('Jan');
		expect(flights.best.length).toBeGreaterThan(0);
		expect(flights.terms.text).not.toBe('');
		expect(source.words().title).toBe('Public flights from here');
	});

	it('reads the candidates of a part of the map, and refuses one too large before asking', async () => {
		const asked: string[] = [];
		const session = await CoreSession.open(new MemoryStore());
		const source = new TakeoffsSource(session, async () => true, server(asked));
		const answer = await source.candidates({
			south: 47.25,
			west: 10.92,
			north: 47.27,
			east: 10.95
		});
		expect(answer.candidates).toHaveLength(1);
		expect(answer.candidates[0].words).toMatch(/^Pilots take off here/);
		expect(answer.candidates[0].altitudeWords).toBe('1620 m');
		await expect(source.candidates({ south: 46, west: 10, north: 48.5, east: 11 })).rejects.toThrow(
			/Zoom in/
		);
		expect(asked).toHaveLength(1);
	});

	it('offers nothing to a browser nobody is signed in to, and sets a place aside', async () => {
		const session = await CoreSession.open(new MemoryStore());
		const source = new TakeoffsSource(session, async () => true, server([]));
		const answer = await source.candidates({
			south: 47.25,
			west: 10.92,
			north: 47.27,
			east: 10.95
		});
		expect(await source.offers(answer, 3600)).toEqual([]);
		await source.setAside(answer.candidates[0].position);
		expect(await source.offers(answer, 3600)).toEqual([]);
	});
});
