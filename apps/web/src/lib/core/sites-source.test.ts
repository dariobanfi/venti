import { readFileSync } from 'node:fs';
import { beforeAll, describe, expect, it } from 'vitest';
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

		const map = (await sites.sites(VALLEY))!;

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

		const map = (await sites.sites(VALLEY))!;

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

		expect((await sites.sites(VALLEY))!.isWhole).toBe(true);
		// More than the server's set is whole for: an empty map there says nothing.
		const europe = { south: 40, west: 0, north: 55, east: 20 };
		expect((await sites.sites(europe))!.isWhole).toBe(false);
	});

	it('are not asked for on a site without a server', async () => {
		const { asked, get } = server();
		const sites = new SitesSource(async () => false, get);

		expect(await sites.sites(VALLEY)).toBeUndefined();
		expect(asked).toEqual([]);
	});

	it('report a server in trouble in the core’s words', async () => {
		const { get } = server(() => ({ status: 502, body: new TextEncoder().encode('Bad Gateway') }));
		const sites = new SitesSource(async () => true, get);

		await expect(sites.sites(VALLEY)).rejects.toMatchObject({
			name: 'Server',
			message: 'The server could not answer (status 502).'
		});
	});

	it('ask for what exists when the map is dragged past the edge of the world', async () => {
		const { asked, get } = server();
		const sites = new SitesSource(async () => true, get);

		await sites.sites({ south: 80, west: 170, north: 95, east: 190 });

		expect(asked).toEqual(['/v1/sites?bbox=170,80,180,90']);
	});
});
