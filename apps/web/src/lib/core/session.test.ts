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

/**
 * The core's logbook, as what the store keeps: the web shows no logbook of
 * its own (owner decision 20), but the phones' core still keeps one, and
 * its files are the largest the store is handed. These tests are about the
 * store.
 */
function logbookIn(session: CoreSession) {
	return {
		importFlight: (text: string) => session.change((core) => core.importFlight(text)),
		flights: async () => (await session.ask((core) => core.logbook())).totals.flights,
		flight: (id: string) => session.ask((core) => core.flight(id))
	};
}

describe('two tabs on one logbook', () => {
	it('shows in one tab what the other imported', async () => {
		browser = new Browser();
		const first = await browser.tab();
		const second = await browser.tab();
		const told = caughtUp(second);

		const { id } = await logbookIn(first).importFlight(sampleFlight());
		await told;

		expect(await logbookIn(second).flights()).toBe(1);
		expect((await logbookIn(second).flight(id)).durationS).toBe(1744);
	});

	it('keeps both changes when two tabs change the logbook at once', async () => {
		browser = new Browser();
		const first = await browser.tab();
		const second = await browser.tab();

		// Neither tab has heard of the other's flight when it stores its own.
		await Promise.all([
			logbookIn(first).importFlight(sampleFlightOn(1)),
			logbookIn(second).importFlight(sampleFlightOn(2))
		]);
		browser.closeTabs();

		const later = await browser.tab();
		expect(await logbookIn(later).flights()).toBe(2);
		expect(await browser.stored()).toHaveLength(3);
	});

	it('says a flight is gone when the other tab deleted it', async () => {
		browser = new Browser();
		const first = await browser.tab();
		const { id } = await logbookIn(first).importFlight(sampleFlight());
		const second = await browser.tab();
		// The second tab does not hear of the deletion before it asks.
		second.onChangedElsewhere = () => {};
		await browser.lose(`flights/${id}.igc`);

		const gone = await logbookIn(second)
			.flight(id)
			.catch((error: Error) => error);

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

		const failure = await logbookIn(session)
			.importFlight(sampleFlight())
			.catch((error: Error) => error);

		expect((failure as Error).name).toBe('QuotaExceededError');
		expect(await logbookIn(session).flights()).toBe(0);
	});

	it('still keeps what it is handed for the visit when there is no storage at all', async () => {
		const session = await CoreSession.open(new MemoryStore());
		closeLater.push(session);

		const { id } = await logbookIn(session).importFlight(sampleFlight());

		expect(session.durable).toBe(false);
		expect(await logbookIn(session).flights()).toBe(1);
		expect((await logbookIn(session).flight(id)).track).toHaveLength(1745);
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

		const failure = await logbookIn(session)
			.importFlight(sampleFlight())
			.catch((error: Error) => error);

		expect((failure as Error).name).toBe('StaleStoreError');
	});
});
