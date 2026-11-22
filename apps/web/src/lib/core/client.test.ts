import { readFileSync } from 'node:fs';
import { beforeAll, describe, expect, it } from 'vitest';
import { AccountSource } from './account-source';
import { AgreementSource } from './agreement-source';
import { FoehnSource } from './foehn-source';
import { AirspaceSource } from './airspace-source';
import { AlertsSource } from './alerts-source';
import { SocialSource } from './social-source';
import { CommunitySource } from './community-source';
import { type Channel, CoreClient } from './client';
import { flightFileText } from './flight-text';
import { ForecastSource } from './forecast-source';
import { GroupsSource } from './groups-source';
import { LiveSource } from './live-source';
import { answer, describeFailure, type Reply, type Request } from './protocol';
import { CoreSession } from './session';
import { SiteEditSource } from './site-edit-source';
import { TakeoffsSource } from './takeoffs-source';
import { SitesSource } from './sites-source';
import { flightFromFile, sitesMap } from './pkg/venti_core_wasm.js';
import { OwnFlightsSource } from './own-flights-source';
import { MemoryStore } from './store';
import { TerrainSource } from './terrain-source';
import { RadarSource } from './radar-source';
import { RouteWeatherSource } from './route-weather-source';
import { CablesSource } from './cables-source';
import { GlideReachSource } from './glide-reach-source';
import { SlopesSource } from './slopes-source';
import { AirSource } from './air-source';
import { ThermalMapSource } from './thermal-map-source';
import { sampleFlight, startCore } from './testing';
import { TodaySource } from './today-source';
import { TokensSource } from './tokens-source';
import { WindSource } from './wind-source';

beforeAll(startCore);

/**
 * The worker's part without a worker: requests go straight to a core in
 * the same process, and replies come back the way a worker's would, after
 * the call has returned.
 */
class SameProcess implements Channel {
	onmessage: ((event: MessageEvent<Reply>) => void) | null = null;
	onerror: ((event: ErrorEvent) => void) | null = null;
	/** What was handed over with each request instead of copied. */
	transferred: Transferable[] = [];
	#session: Promise<CoreSession>;
	#wind: WindSource;

	constructor(session: Promise<CoreSession>, wind = new WindSource()) {
		this.#session = session;
		this.#wind = wind;
	}

	postMessage({ id, call, given }: Request, transfer: Transferable[] = []): void {
		this.transferred.push(...transfer);
		void this.#session
			.then((session) =>
				answer(
					{
						session,
						wind: this.#wind,
						account: account(session),
						tokens: new TokensSource(session, unreachable),
						agreement: new AgreementSource(session, async () => 0, unreachable),
						foehn: new FoehnSource(async () => false),
						alerts: new AlertsSource(session, unreachable),
						social: new SocialSource(session, undefined, unreachable),
						airspace: airspace(session),
						// Flying sites from a site that has no server.
						sites: new SitesSource(async () => false),
						siteEdits: new SiteEditSource(session, () => {}, unreachable),
						takeoffs: new TakeoffsSource(session, async () => false, unreachable),
						forecast: new ForecastSource(async () => false),
						live: new LiveSource(async () => false),
						community: new CommunitySource(async () => false),
						// A browser that is not signed in: no session to ask with.
						own: new OwnFlightsSource(async () => undefined, unreachable, 'https://venti.example'),
						groups: new GroupsSource(session, unreachable),
						// A browser that is offline: no elevation tile is to be had.
						terrain: new TerrainSource(unreachable),
						today: new TodaySource(session, async () => false),
						thermals: new ThermalMapSource(async () => false),
						radar: new RadarSource(async () => false),
						routeWeather: new RouteWeatherSource(async () => false, unreachable, unreachable),
						glideReach: new GlideReachSource(unreachable),
						cables: new CablesSource(async () => false),
						slopes: new SlopesSource(unreachable),
						air: new AirSource(async () => false)
					},
					call,
					given
				)
			)
			.then(
				(result): Reply => ({ id, answer: result }),
				(error): Reply => ({ id, failure: describeFailure(error) })
			)
			.then((reply) => this.onmessage?.({ data: reply } as MessageEvent<Reply>));
	}

	/** What the worker says when another tab has changed the logbook. */
	changeElsewhere(): void {
		this.onmessage?.({ data: { changedElsewhere: true } } as MessageEvent<Reply>);
	}
}

const unreachable = async (): Promise<never> => {
	throw new Error('No server in this test.');
};

/** An account on a server that is never asked in these tests. */
function account(session: CoreSession): AccountSource {
	return new AccountSource(session, unreachable, 'https://venti.example', 'Test browser');
}

/** Airspace from a site that has no server: nothing is ever asked for. */
function airspace(session: CoreSession): AirspaceSource {
	return new AirspaceSource(session, async () => false, unreachable, 'https://venti.example');
}

async function open() {
	const channel = new SameProcess(CoreSession.open(new MemoryStore()));
	return { channel, core: await CoreClient.open(channel) };
}

const bytes = (text: string) => new TextEncoder().encode(text).buffer as ArrayBuffer;

describe('the core as the page calls it', () => {
	it('says how it started', async () => {
		const { core } = await open();

		expect(core.durable).toBe(false);
		expect(core.version).toMatch(/^\d+\.\d+\.\d+$/);
	});

	it('lists, opens and takes in no flight for a browser that is not signed in', async () => {
		const { core, channel } = await open();
		const file = bytes(sampleFlight());

		// The web keeps no logbook of its own: a pilot signs in for flights.
		expect(await core.ownFlights()).toBeUndefined();
		expect(await core.openOwnFlight('0'.repeat(26))).toBeUndefined();
		expect(await core.uploadFlight(file)).toBeUndefined();
		// The file's bytes were handed over, not copied.
		expect(channel.transferred).toContain(file);
	});

	it('answers each call with its own answer, whatever order they finish in', async () => {
		const { core } = await open();

		const [flights, account, files] = await Promise.all([
			core.ownFlights(),
			core.account(),
			core.airspaceFiles()
		]);

		expect(flights).toBeUndefined();
		expect(account).toBeUndefined();
		expect(files).toEqual([]);
	});

	it('brings in an airspace file from its bytes and answers for what a map shows', async () => {
		const { channel, core } = await open();
		const file = bytes(
			'AC D\nAY CTR\nAN CTR TEST\nAL GND\nAH 9000ft AMSL\nV X=47:42:00 N 011:44:00 E\nDC 1\n'
		);

		const imported = await core.importAirspace('zones.txt', file);

		expect(imported).toMatchObject({ airspaces: 1, leftOut: 0 });
		expect(channel.transferred).toContain(file);
		expect(await core.airspaceFiles()).toMatchObject([{ name: 'zones', airspaces: 1 }]);
		const shapes = await core.airspaceIn({ south: 47.6, west: 11.6, north: 47.8, east: 11.9 });
		expect(shapes.map((shape) => [shape.name, shape.policy])).toEqual([['CTR TEST', 'avoid']]);
		// Nothing is kept from a server, and a site without one is not asked.
		expect(await core.keptAirspace()).toEqual([]);
		expect(await core.refreshAirspace({ lat: 47.7, lon: 11.7 })).toBeUndefined();
	});

	it('has no account until the pilot signs in, and says in words why nothing can be done', async () => {
		const { core } = await open();

		expect(await core.account()).toBeUndefined();
		expect(await core.syncKeptSites()).toBeUndefined();
		// The core's refusal crosses to the page with its kind and its words.
		await expect(core.signIn('', '')).rejects.toMatchObject({
			name: 'Server',
			message: 'Enter your handle and your password.'
		});
		await expect(core.signIn('demo', 'demo-pilot-password')).rejects.toThrow(
			'No server in this test.'
		);
	});

	it('has no groups and no plans to ask for until the pilot signs in', async () => {
		const { core } = await open();

		// Refused by the core, in words, before any server is asked.
		await expect(core.planBoard('2026-11-20')).rejects.toMatchObject({
			name: 'Server',
			message: 'This device is not signed in.'
		});
		await expect(core.joinGroup('5555-5555-5555-5555')).rejects.toThrow(
			'This device is not signed in.'
		);
		await expect(
			core.postPlan('44444444444444444444444444', {
				day: '2026-11-21',
				place: 'Musterberg',
				position: undefined,
				meetAt: undefined,
				note: ''
			})
		).rejects.toThrow('This device is not signed in.');
	});

	it('hands a flight’s track over for the ground under it, and has none without tiles', async () => {
		const { core } = await open();
		const flight = flightFromFile(sampleFlight());

		const ground = await core.flightGround({
			asking: 1,
			bounds: flight.bounds,
			track: flight.track
		});

		// No tile could be fetched: the profile stays as it was.
		expect(ground).toEqual([]);
		await core.forgetFlightGround(1);
	});

	it('asks the core what a filter leaves of sites the page holds', async () => {
		const { core } = await open();
		const { sites } = sitesMap(
			47,
			11,
			48,
			12.5,
			200,
			readFileSync(new URL('../../../../../crates/server/tests/golden/sites.json', import.meta.url))
		);

		const choices = await core.sitesFilterChoices();
		expect(choices.shownTitle).toBe('Show');
		expect(choices.windTitle).toBe('Take-offs for a wind from');
		expect(choices.heightTitle).toBe('Take-offs from');

		// The sites and the filter cross to the worker and back as they are.
		const landings = await core.filterSites(sites, {
			shown: 'landings',
			windIs: 'chosen',
			windFromDeg: 315,
			takeoffsFromM: undefined
		});
		expect(landings.fitting.map((site) => site.id)).toEqual([
			'osm-free-flying:node/9908030941',
			'osm-free-flying:way/1080988126'
		]);
		expect(landings.fitting[1].outline).toHaveLength(8);
		expect(landings).toMatchObject({
			unsaid: [],
			words: '2 landings',
			filterWords: 'Landings',
			isOn: true
		});
	});

	it('keeps a site as the pilot’s own, lists it, and forgets it', async () => {
		const { core } = await open();
		const map = sitesMap(
			47,
			11,
			48,
			12.5,
			200,
			readFileSync(new URL('../../../../../crates/server/tests/golden/sites.json', import.meta.url))
		);
		const site = map.sites.find((one) => one.id === 'osm-free-flying:way/1080986173')!;

		expect(await core.isSiteKept(site.id)).toBe(false);
		await core.keepSite(site, map.credits, map.asOfUnixS);
		expect(await core.isSiteKept(site.id)).toBe(true);
		// From a place the list says how far each is; without one it does not.
		expect((await core.keptSites()).sites.map((one) => one.distanceM)).toEqual([undefined]);
		const from = await core.keptSites({ lat: 47.6642, lon: 11.7967 });
		expect(from.sites[0].site.name).toBe('Fluggelände Jägerkamp/Nagelspitz');
		expect(from.sites[0].distanceM!).toBeGreaterThan(5000);
		expect(from.credits).toHaveLength(1);

		await core.forgetSite(site.id);
		expect(await core.isSiteKept(site.id)).toBe(false);
		expect((await core.keptSites()).sites).toEqual([]);
	});

	it('asks the core for the order of Today’s take-offs, a day at a time', async () => {
		const { core } = await open();
		const orders = await core.todayOrders([
			[
				{ id: 'a', isKept: false, distanceM: 1, verdict: 'marginal', thermals: undefined },
				{ id: 'b', isKept: false, distanceM: 2, verdict: 'flyable', thermals: 'local' }
			]
		]);
		expect(orders).toEqual([{ ids: ['b', 'a'], best: 'b' }]);
	});

	it('passes on that another tab changed what the core keeps', async () => {
		const { core, channel } = await open();
		let told = 0;
		core.onChangedElsewhere = () => (told += 1);

		channel.changeElsewhere();

		expect(told).toBe(1);
	});

	it('stops everyone waiting when the worker breaks down', async () => {
		// A worker that never answers.
		const silent: Channel = { postMessage() {}, onmessage: null, onerror: null };
		const opening = CoreClient.open(silent).catch((error: Error) => error);

		silent.onerror?.({ message: 'The script could not be loaded.' } as ErrorEvent);

		expect(((await opening) as Error).message).toBe('The script could not be loaded.');
	});
});

describe('the text of a flight file', () => {
	it('is read as UTF-8', () => {
		expect(flightFileText(new TextEncoder().encode('HFPLTPILOT:Jörg Müller'))).toBe(
			'HFPLTPILOT:Jörg Müller'
		);
	});

	it('is read as Western European text when it is not UTF-8', () => {
		// "Jörg" as an old recorder writes it: one byte for the ö.
		const latin = Uint8Array.from([0x4a, 0xf6, 0x72, 0x67]);
		expect(flightFileText(latin)).toBe('Jörg');
	});

	it('keeps a name that is not UTF-8 when a flight is opened from its file', () => {
		const text = sampleFlight().replace('Sample Pilot', 'Jörg');
		const latin = Uint8Array.from(text, (character) => character.charCodeAt(0));

		expect(flightFromFile(flightFileText(latin)).pilot).toBe('Jörg');
	});
});
