import { IDBFactory } from 'fake-indexeddb';
import { afterEach, beforeAll, describe, expect, it } from 'vitest';
import { IdbStore } from './idb-store';
import { CoreSession } from './session';
import { type FileChange, type FileStore, MemoryStore, type Snapshot } from './store';
import { sampleFlight, sampleFlightOn, startCore } from './testing';

beforeAll(startCore);

/** One browser's storage for the site. Every tab opened on it shares it. */
class Browser {
	#factory = new IDBFactory();
	#open: CoreSession[] = [];

	/** A tab: its own core over the shared store. */
	async tab(): Promise<CoreSession> {
		const session = await CoreSession.open(await IdbStore.open('venti', this.#factory));
		this.#open.push(session);
		return session;
	}

	/** What the database holds, read behind the app's back. */
	async stored(): Promise<string[]> {
		const store = await IdbStore.open('venti', this.#factory);
		const { names } = await store.snapshot();
		store.close();
		return names.sort();
	}

	/** Removes a stored file behind the app's back. */
	async lose(name: string): Promise<void> {
		const store = await IdbStore.open('venti', this.#factory);
		const { revision } = await store.snapshot();
		await store.commit([[name, undefined]], revision);
		store.close();
	}

	closeTabs(): void {
		for (const session of this.#open.splice(0)) session.close();
	}
}

/** Waits until `session` has caught up with a change made in another tab. */
function caughtUp(session: CoreSession): Promise<void> {
	return new Promise((resolve) => {
		session.onChangedElsewhere = resolve;
	});
}

let browser: Browser;
const closeLater: CoreSession[] = [];

afterEach(() => {
	browser?.closeTabs();
	for (const session of closeLater.splice(0)) session.close();
});

describe('a logbook in the browser', () => {
	it('starts empty and stores its index', async () => {
		browser = new Browser();
		const tab = await browser.tab();

		expect(tab.logbook().totals.flights).toBe(0);
		expect(tab.logbook().years).toEqual([]);
		expect(tab.durable).toBe(true);
		expect(tab.version()).toMatch(/^\d+\.\d+\.\d+$/);
		expect(await browser.stored()).toEqual(['logbook-index.json']);
	});

	it('keeps an imported flight across a reload', async () => {
		browser = new Browser();
		const imported = await (await browser.tab()).importFlight(sampleFlight());
		expect(imported.alreadyInLogbook).toBe(false);
		expect(await browser.stored()).toEqual([`flights/${imported.id}.igc`, 'logbook-index.json']);
		browser.closeTabs();

		const reloaded = await browser.tab();
		const logbook = reloaded.logbook();
		expect(logbook.totals).toMatchObject({ flights: 1, airtimeS: 1744 });
		expect(logbook.years.map((year) => year.year)).toEqual([2026]);
		const entry = logbook.years[0].flights[0];
		expect(entry).toMatchObject({ id: imported.id, glider: 'Sample Wing', durationS: 1744 });
		expect(entry.outline.length).toBeGreaterThan(2);
	});

	it('opens a stored flight with everything a screen shows', async () => {
		browser = new Browser();
		const { id } = await (await browser.tab()).importFlight(sampleFlight());
		browser.closeTabs();

		const flight = await (await browser.tab()).flight(id);

		expect(flight).toMatchObject({
			id,
			pilot: 'Sample Pilot',
			glider: 'Sample Wing',
			recorder: 'XVE 000',
			durationS: 1744,
			maxAltitudeM: 2117
		});
		expect(flight.track).toHaveLength(1745);
		expect(flight.track[0]).toMatchObject({ seconds: 0 });
		expect(flight.profile.length).toBeLessThanOrEqual(400);
		expect(flight.bounds.south).toBeLessThan(flight.bounds.north);
	});

	it('hands a stored flight\u2019s file back as it went in, under a recorder\u2019s kind of name', async () => {
		browser = new Browser();
		const { id } = await (await browser.tab()).importFlight(sampleFlight());
		browser.closeTabs();

		const file = await (await browser.tab()).flightFile(id);

		expect(file).toEqual({ name: '2026-07-04-XVE-000-01.igc', igcText: sampleFlight() });
	});

	it('says before a stored flight goes to a league until when the leagues take it, after a reload', async () => {
		browser = new Browser();
		const { id } = await (await browser.tab()).importFlight(sampleFlight());
		browser.closeTabs();

		// The sample flight was flown on 4 July 2026: by the 10th it is in time,
		// by November it is not.
		const tab = await browser.tab();
		const early = await tab.leagueHandover(id, Date.UTC(2026, 6, 10) / 1000);
		expect(early.isLate).toBe(false);
		expect(early.deadline).toContain('until the end of 18 Jul 2026, by UTC');
		expect(early.isSigned).toBe(false);
		expect(early.signature).toContain('Venti recorded this flight and does not sign its files yet');
		expect(early.xcontestClaimUrl).toMatch(/^https:\/\/www\.xcontest\.org\//);
		expect(early.file.igcText).toBe(sampleFlight());
		const late = await tab.leagueHandover(id, Date.UTC(2026, 10, 20) / 1000);
		expect(late.isLate).toBe(true);
	});

	it('says what happened in a stored flight, in positions of its track', async () => {
		browser = new Browser();
		const { id } = await (await browser.tab()).importFlight(sampleFlight());
		browser.closeTabs();
		const tab = await browser.tab();

		const analysis = await tab.flightAnalysis(id);
		const { track } = await tab.flight(id);

		expect(analysis.takeoff).toMatchObject({ trackIndex: 0, seconds: 0 });
		expect(analysis.thermals.length).toBeGreaterThan(0);
		expect(analysis.glides.length).toBeGreaterThan(0);
		for (const thermal of analysis.thermals) {
			expect(thermal.entry.trackIndex).toBeLessThan(thermal.exit.trackIndex);
			expect(thermal.exit.trackIndex).toBeLessThan(track.length);
		}
		expect(['freeFlight', 'freeTriangle', 'faiTriangle']).toContain(analysis.score?.kind);
		expect(analysis.score?.route.length).toBeGreaterThanOrEqual(2);
	});

	it('counts the logbook as a whole', async () => {
		browser = new Browser();
		const tab = await browser.tab();
		expect(tab.statistics().months).toEqual([]);
		const { id } = await tab.importFlight(sampleFlight());

		const statistics = tab.statistics();

		expect(statistics.months).toHaveLength(12);
		expect(statistics.months.at(-1)).toMatchObject({ year: 2026, month: 7, flights: 1 });
		expect(statistics.gliders).toMatchObject([{ glider: 'Sample Wing', flights: 1 }]);
		expect(statistics.longest).toMatchObject({ flightId: id, value: 1744 });
		expect(tab.logbook().years[0].flights[0].score?.distanceM).toBeGreaterThan(1000);
	});

	it('says how the pilot flies, counting every stored flight after a reload', async () => {
		browser = new Browser();
		for (let day = 1; day <= 3; day += 1) {
			await (await browser.tab()).importFlight(sampleFlightOn(day));
		}
		// A tab opened later holds none of the files in memory.
		const later = await browser.tab();

		const tendencies = await later.tendencies();

		expect(tendencies.flights).toBe(3);
		expect(tendencies.thermals).toBeGreaterThan(0);
		expect(tendencies.note).not.toBe('');
		for (const observation of tendencies.observations) {
			expect(observation.title).not.toBe('');
			expect(observation.words).not.toBe('');
		}
	});

	it('recognises a flight that is already there', async () => {
		browser = new Browser();
		const tab = await browser.tab();
		const first = await tab.importFlight(sampleFlight());

		const again = await tab.importFlight(sampleFlight().replace('Sample Pilot', 'Someone Else'));

		expect(again).toEqual({ id: first.id, alreadyInLogbook: true });
		expect(tab.logbook().totals.flights).toBe(1);
	});

	it('refuses a file that is not a flight, with the core’s reason', async () => {
		browser = new Browser();
		const tab = await browser.tab();

		const refusal = await tab.importFlight('hello').catch((error: Error) => error);

		expect(refusal).toBeInstanceOf(Error);
		expect((refusal as Error).name).toBe('InvalidFlight');
		expect((refusal as Error).message).toContain('not a flight file');
		expect(await browser.stored()).toEqual(['logbook-index.json']);
	});

	it('removes a deleted flight from the store', async () => {
		browser = new Browser();
		const tab = await browser.tab();
		const { id } = await tab.importFlight(sampleFlight());

		await tab.deleteFlight(id);

		expect(tab.logbook().years).toEqual([]);
		expect(await browser.stored()).toEqual(['logbook-index.json']);
		const gone = await tab.flight(id).catch((error: Error) => error);
		expect((gone as Error).name).toBe('UnknownFlight');
	});

	it('rebuilds a lost index from the flight files, more than one lot of them', async () => {
		browser = new Browser();
		const tab = await browser.tab();
		// More flights than are fetched at once.
		for (let day = 1; day <= 11; day += 1) await tab.importFlight(sampleFlightOn(day));
		browser.closeTabs();
		await browser.lose('logbook-index.json');

		const reloaded = await browser.tab();

		expect(reloaded.logbook().totals.flights).toBe(11);
		expect(await browser.stored()).toContain('logbook-index.json');
	});

	it('runs calls made at the same moment one after another', async () => {
		browser = new Browser();
		const tab = await browser.tab();

		const imported = await Promise.all(
			[1, 2, 3, 4].map((day) => tab.importFlight(sampleFlightOn(day)))
		);

		expect(new Set(imported.map((flight) => flight.id)).size).toBe(4);
		expect(tab.logbook().totals.flights).toBe(4);
		expect(await browser.stored()).toHaveLength(5);
	});
});

describe('a flight checked against airspace, in the browser', () => {
	/**
	 * Ten miles around the valley of the sample flight, from the ground to
	 * far above it, and the same valley again from a height above ground.
	 */
	const ZONES = new TextEncoder().encode(
		[
			'AC D\nAY CTR\nAN CTR VALLEY\nAL GND\nAH 12000ft AMSL\nV X=47:40:00 N 011:46:00 E\nDC 10\n',
			'AC R\nAN ED-R VALLEY\nAL 1000ft AGL\nAH FL 130\nV X=47:40:00 N 011:46:00 E\nDC 10\n'
		].join('\n')
	);

	it('says nothing was checked while there is no airspace', async () => {
		browser = new Browser();
		const tab = await browser.tab();
		const { id } = await tab.importFlight(sampleFlight());

		expect(await tab.flightAirspace(id)).toEqual({ cover: 'none', crossings: [] });
	});

	it('is checked after a reload against files the store has yet to hand over', async () => {
		browser = new Browser();
		const first = await browser.tab();
		const { id } = await first.importFlight(sampleFlight());
		await first.importAirspace('Valley.txt', ZONES, 1_795_028_700);
		browser.closeTabs();

		// A reload: neither the flight's file nor the airspace file is in
		// memory. An answer worked out from what happens to be there would
		// be "nothing was checked".
		const checked = await (await browser.tab()).flightAirspace(id);

		expect(checked.cover).toBe('whole');
		expect(checked.crossings.map((crossing) => crossing.name)).toEqual([
			'CTR VALLEY',
			'ED-R VALLEY'
		]);
		const [zone, restricted] = checked.crossings;
		expect(zone).toMatchObject({
			kind: 'Class D control zone',
			lower: 'GND',
			upper: '12000 ft AMSL',
			policy: 'avoid',
			origin: 'file',
			source: 'valley'
		});
		// From the first position of the flight to its last.
		expect(zone.entry.trackIndex).toBe(0);
		expect(zone.seconds).toBe(zone.exit.seconds - zone.entry.seconds);
		expect(zone.highestM).toBeGreaterThan(zone.lowestM);
		// Ground to an altitude above the sea: nothing in doubt.
		expect(zone.uncertain).toBeUndefined();
		// A floor above ground, and the app has no terrain: only possibly inside.
		expect(restricted.uncertain).toBe(
			'Possibly clear: the app does not know the height of the ground there.'
		);
	});

	it('follows the airspace as it changes', async () => {
		browser = new Browser();
		const tab = await browser.tab();
		const { id } = await tab.importFlight(sampleFlight());
		await tab.importAirspace('Valley.txt', ZONES, 1_795_028_700);
		expect((await tab.flightAirspace(id)).crossings).toHaveLength(2);

		const [file] = await tab.airspaceFiles();
		await tab.deleteAirspaceFile(file.storedName);

		expect(await tab.flightAirspace(id)).toEqual({ cover: 'none', crossings: [] });
	});
});

describe('two tabs on one logbook', () => {
	it('shows in one tab what the other imported', async () => {
		browser = new Browser();
		const first = await browser.tab();
		const second = await browser.tab();
		const told = caughtUp(second);

		const { id } = await first.importFlight(sampleFlight());
		await told;

		expect(second.logbook().totals.flights).toBe(1);
		expect((await second.flight(id)).durationS).toBe(1744);
	});

	it('keeps both changes when two tabs change the logbook at once', async () => {
		browser = new Browser();
		const first = await browser.tab();
		const second = await browser.tab();

		// Neither tab has heard of the other's flight when it stores its own.
		await Promise.all([
			first.importFlight(sampleFlightOn(1)),
			second.importFlight(sampleFlightOn(2))
		]);
		browser.closeTabs();

		const later = await browser.tab();
		expect(later.logbook().totals.flights).toBe(2);
		expect(await browser.stored()).toHaveLength(3);
	});

	it('says a flight is gone when the other tab deleted it', async () => {
		browser = new Browser();
		const first = await browser.tab();
		const { id } = await first.importFlight(sampleFlight());
		const second = await browser.tab();
		// The second tab does not hear of the deletion before it asks.
		second.onChangedElsewhere = () => {};
		await browser.lose(`flights/${id}.igc`);

		const gone = await second.flight(id).catch((error: Error) => error);

		expect((gone as Error).name).toBe('UnknownFlight');
		expect((gone as Error).message).toContain('no longer in the logbook');
	});
});

describe('a browser that will not store', () => {
	/** A store whose disk is full: it reads, and refuses every write but the first. */
	class FullStore extends MemoryStore {
		writes = 0;

		override async commit(changes: FileChange[], revision: number): Promise<number> {
			this.writes += 1;
			if (this.writes > 1)
				throw new DOMException('The quota has been exceeded.', 'QuotaExceededError');
			return super.commit(changes, revision);
		}
	}

	it('does not list a flight it could not store', async () => {
		const session = await CoreSession.open(new FullStore());
		closeLater.push(session);

		const failure = await session.importFlight(sampleFlight()).catch((error: Error) => error);

		expect((failure as Error).name).toBe('QuotaExceededError');
		expect(session.logbook().totals.flights).toBe(0);
		// The core is usable afterwards.
		expect(session.logbook().years).toEqual([]);
	});

	it('still keeps a logbook for the visit when there is no storage at all', async () => {
		const session = await CoreSession.open(new MemoryStore());
		closeLater.push(session);

		const { id } = await session.importFlight(sampleFlight());

		expect(session.durable).toBe(false);
		expect(session.logbook().totals.flights).toBe(1);
		expect((await session.flight(id)).track).toHaveLength(1745);
	});

	it('gives up on a change when the store never stops changing', async () => {
		/** A store that another tab changes before every write of ours. */
		class BusyStore extends MemoryStore {
			override async snapshot(): Promise<Snapshot> {
				const snapshot = await super.snapshot();
				return { ...snapshot, revision: snapshot.revision - 1 };
			}
		}
		const store: FileStore = new BusyStore();
		const session = await CoreSession.open(store);
		closeLater.push(session);

		const failure = await session.importFlight(sampleFlight()).catch((error: Error) => error);

		expect((failure as Error).name).toBe('StaleStoreError');
	});
});
