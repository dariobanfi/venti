import { readFileSync } from 'node:fs';
import { IDBFactory } from 'fake-indexeddb';
import { afterEach, beforeAll, describe, expect, it } from 'vitest';
import { AirspaceSource } from './airspace-source';
import { IdbStore } from './idb-store';
import { CoreSession } from './session';
import { startCore } from './testing';
import type { Answer, Get } from './wind-source';

beforeAll(startCore);

/**
 * The server's own recorded answer, which its tests keep current: the
 * airspace around Hochfilzen, from openAIP.
 */
const recorded = (): Uint8Array =>
	readFileSync(new URL('../../../../../crates/server/tests/golden/airspace.json', import.meta.url));

const SERVER = 'https://venti.example';

/** 18 November 2026, 19:05 UTC, in milliseconds. */
const NOW = 1_795_028_700_000;

/** Hochfilzen, where the recorded answer's airspaces lie. */
const HOCHFILZEN = { lat: 47.49, lon: 12.66 };

/** A box around it, for a map. */
const AROUND = { south: 47.2, west: 12.3, north: 47.8, east: 13.0 };

/** One browser's storage for the site, a server, and a clock to set. */
class Browser {
	#factory = new IDBFactory();
	#open: CoreSession[] = [];
	asked: string[] = [];
	answer: () => Answer = () => ({ status: 200, body: recorded() });
	now = NOW;
	hasServer = true;

	get: Get = async (path) => {
		this.asked.push(path);
		return this.answer();
	};

	async tab(): Promise<{ session: CoreSession; airspace: AirspaceSource }> {
		const session = await CoreSession.open(await IdbStore.open('venti', this.#factory));
		this.#open.push(session);
		const airspace = new AirspaceSource(
			session,
			async () => this.hasServer,
			this.get,
			SERVER,
			() => this.now
		);
		return { session, airspace };
	}

	closeTabs(): void {
		for (const session of this.#open.splice(0)) session.close();
	}
}

let browser: Browser;
afterEach(() => browser?.closeTabs());

describe('airspace from the server', () => {
	it('is asked for around a place, kept, and drawn from then on without the server', async () => {
		browser = new Browser();
		const { session, airspace } = await browser.tab();
		expect(await airspace.kept()).toEqual([]);
		expect(await session.airspaceIn(AROUND)).toEqual([]);

		const kept = (await airspace.refresh(HOCHFILZEN))!;

		// The core asks for two degrees of latitude and three of longitude to each side.
		expect(browser.asked).toEqual(['/v1/airspace?bbox=9.66,45.49,15.66,49.49']);
		expect(kept).toHaveLength(1);
		expect(kept[0]).toMatchObject({ source: 'openaip', provider: 'openAIP', airspaces: 2 });
		// The provider's credit and what it asks to be said travel with it.
		expect(kept[0].credit?.text).toContain('openAIP');
		expect(kept[0].credit?.notice).toBeTruthy();
		const shapes = await session.airspaceIn(AROUND);
		expect(shapes).toHaveLength(2);
		expect(shapes[0].outline.length).toBeGreaterThan(3);

		// After a reload it is still there, and nothing is asked.
		browser.closeTabs();
		browser.asked = [];
		const reloaded = await browser.tab();
		expect(await reloaded.airspace.kept()).toHaveLength(1);
		expect(await reloaded.session.airspaceIn(AROUND)).toHaveLength(2);
		expect(browser.asked).toEqual([]);
	});

	it('is not asked for again while the core says that what is kept will do', async () => {
		browser = new Browser();
		const { airspace } = await browser.tab();
		await airspace.refresh(HOCHFILZEN);
		browser.asked = [];

		// A moment later, a little further on: nothing is sent.
		browser.now += 60_000;
		expect(await airspace.refresh({ lat: 47.6, lon: 12.4 })).toBeUndefined();
		expect(browser.asked).toEqual([]);

		// A day later it is.
		browser.now += 24 * 3600_000;
		expect(await airspace.refresh(HOCHFILZEN)).toHaveLength(1);
		expect(browser.asked).toHaveLength(1);

		// And at once from somewhere far from what is kept.
		browser.asked = [];
		await airspace.refresh({ lat: 46.0, lon: 7.6 });
		expect(browser.asked).toEqual(['/v1/airspace?bbox=4.6,44,10.6,48']);
	});

	it('stays as it was when the server refuses, and the refusal is in the core’s words', async () => {
		browser = new Browser();
		const { session, airspace } = await browser.tab();
		await airspace.refresh(HOCHFILZEN);
		browser.now += 25 * 3600_000;
		browser.answer = () => ({ status: 503, body: new TextEncoder().encode('{}') });

		await expect(airspace.refresh(HOCHFILZEN)).rejects.toMatchObject({
			name: 'Server',
			message: 'The server is busy or being restarted. Try again in a moment.'
		});

		expect(await airspace.kept()).toHaveLength(1);
		expect(await session.airspaceIn(AROUND)).toHaveLength(2);
	});

	it('is forgotten when the pilot switches it off', async () => {
		browser = new Browser();
		const { session, airspace } = await browser.tab();
		await airspace.refresh(HOCHFILZEN);

		await airspace.forget();

		expect(await airspace.kept()).toEqual([]);
		expect(await session.airspaceIn(AROUND)).toEqual([]);
		// Nothing of it is left in the browser's store either.
		browser.closeTabs();
		const reloaded = await browser.tab();
		expect(await reloaded.airspace.kept()).toEqual([]);
	});

	it('is never asked of a site that has no server', async () => {
		browser = new Browser();
		browser.hasServer = false;
		const { airspace } = await browser.tab();

		expect(await airspace.refresh(HOCHFILZEN)).toBeUndefined();
		expect(browser.asked).toEqual([]);
	});

	it('is drawn beside the pilot’s own files, each airspace once', async () => {
		browser = new Browser();
		const { session, airspace } = await browser.tab();
		const file = new TextEncoder().encode(
			'AC D\nAY CTR\nAN CTR TEST\nAL GND\nAH 9000ft AMSL\nV X=47:30:00 N 012:40:00 E\nDC 1\n'
		);
		const imported = await session.importAirspace('Test Zones.txt', file, NOW / 1000);
		expect(imported).toMatchObject({ airspaces: 1, leftOut: 0 });
		await airspace.refresh(HOCHFILZEN);

		const shapes = await session.airspaceIn(AROUND);

		expect(shapes).toHaveLength(3);
		expect(shapes.map((shape) => shape.name)).toContain('CTR TEST');
		expect(await session.airspaceFiles()).toMatchObject([{ name: 'test-zones', airspaces: 1 }]);

		// A second tab sees both after it has caught up.
		const second = await browser.tab();
		expect(await second.session.airspaceIn(AROUND)).toHaveLength(3);
		await session.deleteAirspaceFile('20261118-test-zones.txt');
		await second.session.reload();
		expect(await second.session.airspaceIn(AROUND)).toHaveLength(2);
	});
});
