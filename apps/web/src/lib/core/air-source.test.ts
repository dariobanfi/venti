import { readFileSync } from 'node:fs';
import { beforeAll, describe, expect, it } from 'vitest';
import { AirSource } from './air-source';
import { startCore } from './testing';
import type { Get } from './wind-source';

beforeAll(startCore);

/** One of the server's own recorded answers, by the name of its golden file. */
const recorded = (name: string): Uint8Array =>
	readFileSync(new URL(`../../../../../crates/server/tests/golden/${name}.json`, import.meta.url));

/** The Tegernsee valley, well inside what the server answers for. */
const VALLEY = { south: 47.6, west: 11.7, north: 47.75, east: 11.95 };

/** Answers every request with one recorded file, and keeps what was asked. */
function answering(name: string): { get: Get; asked: string[] } {
	const asked: string[] = [];
	return {
		asked,
		get: async (path) => {
			asked.push(path);
			return { status: 200, body: recorded(name) };
		}
	};
}

const never: Get = async () => {
	throw new Error('asked');
};

describe('what many gliders say together, from the server', () => {
	it('reads the climbs of a part of the map, each with the core’s label and line', async () => {
		const { get, asked } = answering('live_climbs');
		const climbs = (await new AirSource(async () => true, get).climbs(VALLEY))!;

		expect(asked).toHaveLength(1);
		expect(asked[0]).toMatch(/^\/v1\/live\/climbs\?bbox=/);
		expect(climbs.climbs.map((climb) => climb.label)).toEqual(['+1.3', '+2.4']);
		expect(climbs.climbs[1].line).toMatch(
			/^Climbing now: \+2\.4 m\/s between 1,600 and 2,100 m, 3 pilots, 4 min ago$/
		);
		expect(climbs.countWords).toBe('2 climbs in the last 10 minutes');
		expect(climbs.credits.map((credit) => credit.text)).toEqual([
			'Positions: Open Glider Network, glidernet.org (ODbL)'
		]);
		// What the map says where the layer is on and empty comes with the answer.
		expect(climbs.emptyWords).not.toBe('');

		// Half a continent, wider than the server answers for: nothing is
		// asked, and the core says to zoom in.
		const wide = (await new AirSource(async () => true, get).climbs({
			south: 36,
			west: -5,
			north: 60,
			east: 25
		}))!;
		expect(asked).toHaveLength(1);
		expect(wide.climbs).toEqual([]);
		expect(wide.emptyWords).toBe('Zoom in to see where pilots climb now.');
	});

	it('says what the layers’ sheet says, in the core’s words', () => {
		const words = new AirSource().words();
		expect(words.title).toBe('Layers');
		expect(words.onTheMap).toBe('On the map');
		expect(words.listsTitle).toBe('Lists and plans');
		expect(words.climbsNow).toBe('Climbs now');
		expect(words.flownThermals).toBe('Where thermals have been');
		expect(words.alarmDayKind).toBe('Flying day');
	});

	it('asks for the thermals of every month, and says in the core’s words when the map shows too much', async () => {
		const { get, asked } = answering('thermals_flown');
		const source = new AirSource(async () => true, get);

		const flown = (await source.flown(VALLEY))!;
		expect(asked).toHaveLength(1);
		expect(asked[0]).toMatch(/^\/v1\/thermals\/flown\?bbox=/);
		expect(asked[0]).not.toMatch(/month|hours/);
		expect(flown.thermals?.cells).toHaveLength(2);
		expect(flown.thermals?.cells[0].line).toMatch(/^5 climbs of 3 pilots, \+2\.4 m\/s on average/);
		expect(flown.thermals?.note).toMatch(/^Where pilots climbed, from the public flights on Venti/);
		expect(flown.thermals?.terms).toMatch(/^These flights are their pilots'/);
		expect(flown.thermals?.termsTitle).toBe('Terms for these flights');

		// The whole northern Alps: nothing is asked.
		const wide = await source.flown({ south: 46, west: 9, north: 49, east: 14 });
		expect(wide).toEqual({ tooWide: 'Zoom in to see where pilots climbed.' });
		expect(asked).toHaveLength(1);
	});

	it('says who flies at a site, with its name where it has one, and nothing where nobody does', async () => {
		const { get, asked } = answering('live_sites');
		const source = new AirSource(async () => true, get);
		const ramp = { id: 'osm-free-flying:node/5964676196', lat: 47.6652, lon: 11.7954 };

		expect(await source.siteAir({ ...ramp, name: 'Wallberg' })).toBe(
			'8 pilots in the air at Wallberg, 2 took off in the last hour'
		);
		// A site its source does not name: the core's "here", never a word of the app's.
		expect(await source.siteAir({ ...ramp, name: '' })).toBe(
			'8 pilots in the air here, 2 took off in the last hour'
		);
		expect(asked[0]).toMatch(/^\/v1\/live\/sites\?bbox=/);
		expect(await source.siteAir({ ...ramp, id: 'osm-free-flying:way/1', name: 'Elsewhere' })).toBe(
			''
		);
	});

	it('asks nothing where the site has no server', async () => {
		const source = new AirSource(async () => false, never);
		expect(await source.climbs(VALLEY)).toBe(undefined);
		expect(await source.flown(VALLEY)).toBe(undefined);
		expect(await source.siteAir({ id: 'a', name: 'A', lat: 47.66, lon: 11.79 })).toBe('');
	});
});
