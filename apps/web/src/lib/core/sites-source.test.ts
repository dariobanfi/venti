import { readFileSync } from 'node:fs';
import { beforeAll, describe, expect, it } from 'vitest';
import type { SitesFilter } from './pkg/venti_core_wasm.js';
import { SitesSource } from './sites-source';
import { startCore } from './testing';
import type { Answer, Get } from './wind-source';

beforeAll(startCore);

/**
 * The server's own recorded answer, which its tests keep current: the five
 * sites OpenStreetMap had around the Tegernsee on 18 November 2026.
 */
const recorded = (): Uint8Array =>
	readFileSync(new URL('../../../../../crates/server/tests/golden/sites.json', import.meta.url));

/** The valley of the sample flight, with the Spitzingsee east of it. */
const VALLEY = { south: 47.6, west: 11.7, north: 47.75, east: 11.95 };

/** Nothing chosen: every site, without the places from pilots. */
const NO_FILTER: SitesFilter = {
	shown: 'all',
	windIs: 'chosen',
	windFromDeg: undefined,
	takeoffsFromM: undefined
};

/** A server that gives one answer and notes what it was asked. */
function server(answer: () => Answer = () => ({ status: 200, body: recorded() })) {
	const asked: string[] = [];
	const get: Get = async (path) => {
		asked.push(path);
		return answer();
	};
	return { asked, get };
}

describe('flying sites from the server', () => {
	it('are asked for by the box of the map, and read as the source gives them', async () => {
		const { asked, get } = server();
		const sites = new SitesSource(async () => true, get);

		const map = (await sites.sites(VALLEY, NO_FILTER))!;

		expect(asked).toEqual(['/v1/sites?bbox=11.7,47.6,11.95,47.75']);
		expect(map.sites).toHaveLength(5);
		// Most have no name: they are called what they are.
		expect(map.sites[0]).toMatchObject({
			id: 'osm-free-flying:node/5964676196',
			name: 'Take-off',
			isNamed: false,
			kind: 'takeoff',
			wind: [],
			windInWords: '',
			discouraged: false,
			source: 'osm-free-flying',
			url: 'https://www.openstreetmap.org/node/5964676196'
		});
		// What the source does not say is left out, not made up.
		expect(map.sites[0].elevationM).toBeUndefined();
		expect(map.sites[0].official).toBeUndefined();
		expect(map.sites.find((site) => site.isNamed && site.kind === 'takeoff')).toMatchObject({
			name: 'Fluggelände Jägerkamp/Nagelspitz',
			kindInWords: 'Take-off',
			elevationM: 1560,
			official: true,
			paragliding: true
		});
		expect(map.sites.filter((site) => site.kind === 'landing')).toHaveLength(2);
	});

	it('come with the credit and the notice of their source, and the date of its data', async () => {
		const { get } = server();
		const sites = new SitesSource(async () => true, get);

		const map = (await sites.sites(VALLEY, NO_FILTER))!;

		expect(map.credits).toEqual([
			{
				source: 'osm-free-flying',
				provider: 'OpenStreetMap contributors',
				licence: 'ODbL 1.0',
				text: '© OpenStreetMap contributors',
				url: 'https://www.openstreetmap.org/copyright',
				notice: expect.stringContaining('https://opendatacommons.org/licenses/odbl/1-0/')
			}
		]);
		// 18 November 2026, 19:05 UTC: a number, not a BigInt.
		expect(map.asOfUnixS).toBe(1_795_028_700);
	});

	it('say whether the server holds everything its sources have for the box', async () => {
		const { get } = server();
		const sites = new SitesSource(async () => true, get);

		expect((await sites.sites(VALLEY, NO_FILTER))!.isWhole).toBe(true);
		// More than the server's set is whole for: an empty map there says nothing.
		const europe = { south: 40, west: 0, north: 55, east: 20 };
		expect((await sites.sites(europe, NO_FILTER))!.isWhole).toBe(false);
	});

	it('are filtered by the core, which says in words what a filter leaves', async () => {
		const { asked, get } = server();
		const sites = new SitesSource(async () => true, get);
		const held = (await sites.sites(VALLEY, NO_FILTER))!.sites;
		asked.length = 0;

		const choices = sites.filterChoices();
		expect(choices.shown.map((one) => [one.shown, one.words, one.asksTakeoffs])).toEqual([
			['all', 'All sites', true],
			['takeoffs', 'Take-offs', true],
			['landings', 'Landings', false]
		]);
		expect(choices.winds).toHaveLength(16);
		expect(choices.winds[14]).toEqual({ fromDeg: 315, words: 'NW' });
		expect(choices.windsNow.map((one) => one.windIs)).toEqual(['forecastNow', 'measuredNow']);
		// "Any" asks about no height: nothing, not a number.
		expect(choices.heights.map((one) => [one.fromM, one.words])).toEqual([
			[undefined, 'Any'],
			[1000, '1000 m'],
			[1500, '1500 m'],
			[2000, '2000 m']
		]);

		// Nothing asked: every site, and the filter is not on.
		const all = sites.filter(held, {
			shown: 'all',
			windIs: 'chosen',
			windFromDeg: undefined,
			takeoffsFromM: undefined
		});
		expect(all.fitting).toEqual(held);
		expect(all).toMatchObject({
			unsaid: [],
			words: '5 sites',
			filterWords: 'All sites',
			isOn: false
		});

		// None of the valley's three take-offs names a wind: none fits, none is
		// left out, and the landings are not shown.
		const northWest = sites.filter(held, {
			shown: 'takeoffs',
			windIs: 'chosen',
			windFromDeg: 315,
			takeoffsFromM: undefined
		});
		expect(northWest.fitting).toEqual([]);
		expect(northWest.unsaid.map((site) => site.id)).toEqual([
			'osm-free-flying:node/5964676196',
			'osm-free-flying:node/5964676197',
			'osm-free-flying:way/1080986173'
		]);
		expect(northWest).toMatchObject({
			words: '0 of 3 take-offs fit a wind from NW',
			unsaidTitle: '3 take-offs give no wind directions',
			filterWords: 'Take-offs for a wind from NW',
			isOn: true
		});
		expect(northWest.unsaidNote).toContain('shown apart, not left out');

		// One gives its height, 1560 m; the wind a station measured is worded as that.
		const high = sites.filter(held, {
			shown: 'all',
			windIs: 'measuredNow',
			windFromDeg: undefined,
			takeoffsFromM: 1500
		});
		expect(high.fitting.map((site) => site.name)).toEqual([
			'Landing',
			'Fluggelände Jägerkamp/Nagelspitz',
			'Landeplatz Jägerkamp/Nagelspitz'
		]);
		expect(high.words).toBe('1 of 3 take-offs lies at 1500 m or higher');
		expect(high.unsaidTitle).toBe('2 take-offs give no height');
		// The outline of a site mapped as an area comes through the filter.
		expect(high.fitting.map((site) => site.outline.length)).toEqual([0, 5, 8]);
		// The core decides by itself: the server is not asked.
		expect(asked).toEqual([]);
	});

	it('are not asked for on a site without a server', async () => {
		const { asked, get } = server();
		const sites = new SitesSource(async () => false, get);

		expect(await sites.sites(VALLEY, NO_FILTER)).toBeUndefined();
		expect(asked).toEqual([]);
	});

	it('report a server in trouble in the core’s words', async () => {
		const { get } = server(() => ({ status: 502, body: new TextEncoder().encode('Bad Gateway') }));
		const sites = new SitesSource(async () => true, get);

		await expect(sites.sites(VALLEY, NO_FILTER)).rejects.toMatchObject({
			name: 'Server',
			message: 'The server could not answer (status 502).'
		});
	});

	it('ask for what exists when the map is dragged past the edge of the world', async () => {
		const { asked, get } = server();
		const sites = new SitesSource(async () => true, get);

		await sites.sites({ south: 80, west: 170, north: 95, east: 190 }, NO_FILTER);

		expect(asked).toEqual(['/v1/sites?bbox=170,80,180,90']);
	});
});

describe('the flying site at a place', () => {
	/** Where the development mode's flights take off, beside the Wallberg's ramps. */
	const TAKEOFF = { lat: 47.6642, lon: 11.7967 };

	it('is the nearest take-off within a kilometre, with its source’s credit', async () => {
		const { asked, get } = server();
		const sites = new SitesSource(async () => true, get);

		const found = (await sites.siteAt(TAKEOFF))!;

		expect(asked).toHaveLength(1);
		expect(asked[0]).toMatch(/^\/v1\/sites\?bbox=/);
		expect(found.site).toMatchObject({ id: 'osm-free-flying:node/5964676196', kind: 'takeoff' });
		expect(found.credit?.text).toBe('© OpenStreetMap contributors');
		expect(typeof found.asOfUnixS).toBe('number');
	});

	it('is the one named, and none where the source has none', async () => {
		const sites = new SitesSource(async () => true, server().get);

		const named = (await sites.siteAt(TAKEOFF, 'osm-free-flying:node/5964676197'))!;
		expect(named.site?.id).toBe('osm-free-flying:node/5964676197');

		// The landing field in the valley is no take-off.
		const valley = (await sites.siteAt({ lat: 47.6919, lon: 11.888 }))!;
		expect(valley.site).toBeUndefined();
		expect(valley.credit).toBeUndefined();
	});

	it('is not asked for on a site without a server', async () => {
		const { asked, get } = server();
		expect(await new SitesSource(async () => false, get).siteAt(TAKEOFF)).toBeUndefined();
		expect(asked).toEqual([]);
	});
});
