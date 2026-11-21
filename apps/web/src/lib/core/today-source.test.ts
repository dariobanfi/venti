import { readFileSync } from 'node:fs';
import { IDBFactory } from 'fake-indexeddb';
import { afterEach, beforeAll, describe, expect, it } from 'vitest';
import { IdbStore } from './idb-store';
import { sitesMap, type SitesMap } from './pkg/venti_core_wasm.js';
import { CoreSession } from './session';
import { startCore } from './testing';
import { todayOrders, TodaySource } from './today-source';
import type { Answer, Get } from './wind-source';

beforeAll(startCore);

const recorded = (name: string): Uint8Array =>
	readFileSync(new URL(`../../../../../crates/server/tests/golden/${name}`, import.meta.url));

/** Where the development mode's flights take off, beside the Wallberg's ramps. */
const TEGERNSEE = { lat: 47.6642, lon: 11.7967 };
/** A place far from every recorded site. */
const HAMBURG = { lat: 53.55, lon: 10.0 };
/** The evening of 18 November 2026: the server's recorded forecast is of then. */
const NOW = Date.UTC(2026, 10, 18, 8) / 1000;

/**
 * One browser's storage, and a server with recorded sites and forecast:
 * the forecast "not yet" for the first `busy` asks, the sites not at all
 * while `sitesDown`.
 */
class Browser {
	#factory = new IDBFactory();
	#open: CoreSession[] = [];
	asked: string[] = [];
	busy = 0;
	sitesDown = false;

	get: Get = async (path): Promise<Answer> => {
		this.asked.push(path);
		if (path.startsWith('/v1/sites')) {
			return this.sitesDown
				? { status: 503, body: new Uint8Array() }
				: { status: 200, body: recorded('sites.json') };
		}
		if (this.busy > 0) {
			this.busy -= 1;
			return { status: 503, body: new Uint8Array() };
		}
		return { status: 200, body: recorded('forecast.json') };
	};

	/** A tab over the browser's storage: opened again, it is the page after a reload. */
	async tab(hasServer = true): Promise<{ session: CoreSession; today: TodaySource }> {
		const session = await CoreSession.open(await IdbStore.open('venti', this.#factory));
		this.#open.push(session);
		const today = new TodaySource(
			session,
			async () => hasServer,
			async () => NOW,
			this.get,
			0
		);
		return { session, today };
	}

	closeTabs(): void {
		for (const session of this.#open.splice(0)) session.close();
	}
}

/** The recorded sites as a map answers them, to keep one from. */
function recordedMap(): SitesMap {
	return sitesMap(47, 11, 48, 12.5, 200, recorded('sites.json'));
}

let browser: Browser;
afterEach(() => browser?.closeTabs());

describe('today', () => {
	it('lists the take-offs the core chooses around a place, under the core’s names', async () => {
		browser = new Browser();
		const { today } = await browser.tab();
		const found = (await today.takeoffs(TEGERNSEE))!;
		expect(found.nowUnixS).toBe(NOW);
		expect(found.nearProblem).toBeUndefined();
		// The core's rule: named ones first, then the nearest.
		expect(found.listed.takeoffs.map((takeoff) => takeoff.site.name)).toEqual([
			'Fluggelände Jägerkamp/Nagelspitz',
			'Take-off',
			'Take-off'
		]);
		expect(found.listed.takeoffs.every((takeoff) => !takeoff.isKept)).toBe(true);
		expect(found.listed.takeoffs[1].distanceM!).toBeLessThan(found.listed.takeoffs[2].distanceM!);
		expect(found.listed.keptTitle).toBe('Your take-offs');
		expect(found.listed.nearTitle).toBe('Within 30 km');
		expect(found.listed.keptMore).toBe('');
		expect(found.listed.credits.map((credit) => credit.source)).toEqual(['osm-free-flying']);
		// The box asked for is the core's, which holds the 30 km.
		expect(browser.asked[0]).toMatch(/^\/v1\/sites\?/);
	});

	it('asks again for a forecast the server is still fetching, and reads what it says of a take-off', async () => {
		browser = new Browser();
		browser.busy = 2;
		const { today } = await browser.tab();
		const { listed } = (await today.takeoffs(TEGERNSEE))!;
		const outlook = await today.outlook(listed.takeoffs[0].site.id, NOW);
		expect(browser.asked.filter((path) => path.startsWith('/v1/forecast'))).toHaveLength(3);
		// The recorded answer holds one hour of an evening: a model and its
		// note are read, and no day, since none has an hour of daylight.
		expect(outlook.model).toBe('ICON-D2');
		expect(outlook.note).not.toBe('');
		expect(outlook.days).toEqual([]);
	});

	it('lists a kept take-off first, however far the place, and still after a reload', async () => {
		browser = new Browser();
		const first = await browser.tab();
		const map = recordedMap();
		const ramp = map.sites.find((site) => site.id === 'osm-free-flying:node/5964676196')!;
		expect(await first.session.isSiteKept(ramp.id)).toBe(false);
		await first.session.keepSite(ramp, map.credits, map.asOfUnixS);
		expect(await first.session.isSiteKept(ramp.id)).toBe(true);

		// Around the Tegernsee it leads, and is not listed a second time.
		const near = (await first.today.takeoffs(TEGERNSEE))!.listed;
		expect(near.takeoffs.map((takeoff) => [takeoff.site.id, takeoff.isKept])).toEqual([
			[ramp.id, true],
			['osm-free-flying:way/1080986173', false],
			['osm-free-flying:node/5964676197', false]
		]);

		// The page is reloaded: the browser's store is all there is.
		browser.closeTabs();
		const second = await browser.tab();
		expect(await second.session.isSiteKept(ramp.id)).toBe(true);
		const kept = await second.session.keptSites(undefined);
		expect(kept.sites.map((one) => one.site.id)).toEqual([ramp.id]);
		expect(kept.sites[0].asOfUnixS).toBe(map.asOfUnixS);
		expect(kept.sites[0].site.description).toBe('Startrampe (vermutlich für Drachenflieger)');
		expect(kept.note).toMatch(/^Kept on this device only/);
		expect(kept.credits.map((credit) => credit.source)).toEqual(['osm-free-flying']);

		// Far from every site it is the one take-off, with how far away.
		const far = (await second.today.takeoffs(HAMBURG))!.listed;
		expect(far.takeoffs.map((takeoff) => takeoff.site.id)).toEqual([ramp.id]);
		expect(far.takeoffs[0].distanceM!).toBeGreaterThan(500_000);
		// And its forecast can be asked for.
		expect((await second.today.outlook(ramp.id, NOW)).model).toBe('ICON-D2');
	});

	it('lists the pilot’s own alone when Venti gives no sites, and fails in words with none kept', async () => {
		browser = new Browser();
		browser.sitesDown = true;
		const { session, today } = await browser.tab();
		await expect(today.takeoffs(TEGERNSEE)).rejects.toThrow();

		const map = recordedMap();
		const jaegerkamp = map.sites.find((site) => site.id === 'osm-free-flying:way/1080986173')!;
		await session.keepSite(jaegerkamp, map.credits, map.asOfUnixS);
		const found = (await today.takeoffs(TEGERNSEE))!;
		expect(found.listed.takeoffs.map((takeoff) => takeoff.site.id)).toEqual([jaegerkamp.id]);
		expect(found.nearProblem).not.toBe('');
		expect(found.nearProblem).toBeDefined();
		// The credit kept with the site stands in for the answer's.
		expect(found.listed.credits.map((credit) => credit.source)).toEqual(['osm-free-flying']);

		await session.forgetSite(jaegerkamp.id);
		expect(await session.isSiteKept(jaegerkamp.id)).toBe(false);
		await expect(today.takeoffs(TEGERNSEE)).rejects.toThrow();
	});

	it('hands each day’s standings to the core for its order', () => {
		const standing = (id: string, isKept: boolean, verdict?: 'flyable' | 'notFlyable') => ({
			id,
			isKept,
			distanceM: 1000,
			verdict,
			thermals: undefined
		});
		const orders = todayOrders([
			[standing('near', false, 'flyable'), standing('own', true, 'notFlyable')],
			[standing('near', false), standing('own', true)]
		]);
		// The pilot's own first in the list; the best is the best of all.
		expect(orders.map((order) => order.ids)).toEqual([
			['own', 'near'],
			['own', 'near']
		]);
		expect(orders[0].best).toBe('near');
		expect(orders[1].best).toBeUndefined();
	});

	it('is not asked for on a site without a server', async () => {
		browser = new Browser();
		const { today } = await browser.tab(false);
		expect(await today.takeoffs(TEGERNSEE)).toBe(undefined);
		expect(browser.asked).toEqual([]);
	});
});
