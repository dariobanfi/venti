/**
 * The Rust core with a browser's store behind it.
 *
 * The core's calls do not wait, and a browser's storage always makes a page
 * wait. This is where the two meet. The core says which stored files it
 * needs and which changes it wants stored; this class does the reading and
 * the writing and repeats the call. `crates/core-wasm` describes the core's
 * side.
 *
 * Nothing here knows what a flight is. Every rule about the logbook is in
 * the core.
 */

import { Core, StoredFiles } from './pkg/venti_core_wasm.js';
import type {
	FlightAnalysis,
	FlightFile,
	FlightView,
	ImportedFlight,
	LogbookStatistics,
	LogbookView
} from './pkg/venti_core_wasm.js';
import { type FileStore, StaleStoreError } from './store';

/**
 * How many flight files are fetched before the core takes them into a
 * logbook whose index was lost. It bounds what is in memory at once.
 */
const FETCHED_AT_ONCE = 8;

/** How often a change is tried again when another tab keeps changing the store first. */
const ATTEMPTS = 4;

export class CoreSession {
	#store: FileStore;
	#files!: StoredFiles;
	#core!: Core;
	/** The store's revision that `#files` describes. */
	#revision = 0;
	/** The end of the queue of calls: they run one at a time. */
	#last: Promise<unknown> = Promise.resolve();
	#stopListening: () => void;

	/** Called after another tab changed the store and this one has caught up. */
	onChangedElsewhere: () => void = () => {};

	private constructor(store: FileStore) {
		this.#store = store;
		this.#stopListening = store.onChangedElsewhere(() => {
			// If the store cannot be read now, the page keeps what it has.
			void this.reload().then(
				() => this.onChangedElsewhere(),
				() => {}
			);
		});
	}

	/** Starts the core over what `store` holds. The WebAssembly module must be loaded. */
	static async open(store: FileStore): Promise<CoreSession> {
		const session = new CoreSession(store);
		await session.reload();
		return session;
	}

	/** Whether the logbook is still there after the page is closed. */
	get durable(): boolean {
		return this.#store.durable;
	}

	version(): string {
		return this.#core.version();
	}

	/** The logbook as it is now. */
	logbook(): LogbookView {
		return this.#core.logbook();
	}

	/** The logbook looked at as a whole. */
	statistics(): LogbookStatistics {
		return this.#core.statistics();
	}

	importFlight(igcText: string): Promise<ImportedFlight> {
		return this.#change((core) => core.importFlight(igcText));
	}

	flight(id: string): Promise<FlightView> {
		return this.#inTurn(() => fetching(this.#files, this.#store, () => this.#core.flight(id)));
	}

	flightAnalysis(id: string): Promise<FlightAnalysis> {
		return this.#inTurn(() =>
			fetching(this.#files, this.#store, () => this.#core.flightAnalysis(id))
		);
	}

	flightFile(id: string): Promise<FlightFile> {
		return this.#inTurn(() => fetching(this.#files, this.#store, () => this.#core.flightFile(id)));
	}

	deleteFlight(id: string): Promise<void> {
		return this.#change((core) => core.deleteFlight(id));
	}

	/** Starts again from what is stored. */
	reload(): Promise<void> {
		return this.#inTurn(() => this.#load());
	}

	close(): void {
		this.#stopListening();
		this.#core.free();
		this.#files.free();
		this.#store.close();
	}

	/**
	 * Makes a call that changes the logbook and stores the change. Only
	 * when it is stored does the caller hear that it worked.
	 */
	#change<T>(call: (core: Core) => T): Promise<T> {
		return this.#inTurn(async () => {
			for (let attempt = 1; ; attempt += 1) {
				const result = await fetching(this.#files, this.#store, () => call(this.#core));
				try {
					await this.#commit();
					return result;
				} catch (error) {
					// The core now believes in a change the store does not
					// hold. Start again from the store, whatever went wrong.
					await this.#load();
					// Another tab got there first: its change is in now, so
					// the call is made again and may well turn out differently.
					if (!(error instanceof StaleStoreError) || attempt === ATTEMPTS) throw error;
				}
			}
		});
	}

	/** Hands the core's staged changes to the store. */
	async #commit(): Promise<void> {
		const changes = this.#files.takeStaged();
		if (changes.length > 0) {
			this.#revision = await this.#store.commit(changes, this.#revision);
		}
	}

	/** Builds the core anew from the store and puts it in place of the old one. */
	async #load(): Promise<void> {
		const { names, revision } = await this.#store.snapshot();
		const files = new StoredFiles(names);
		let core: Core;
		try {
			core = await fetching(files, this.#store, () => new Core(files));
			await takeInUnlisted(files, core, this.#store);
		} catch (error) {
			files.free();
			throw error;
		}

		const old: { core?: Core; files?: StoredFiles } = { core: this.#core, files: this.#files };
		this.#files = files;
		this.#core = core;
		this.#revision = revision;
		old.core?.free();
		old.files?.free();

		try {
			// A new logbook's empty index, or an index that was rebuilt.
			await this.#commit();
		} catch {
			// The index is a cache. If it cannot be stored now, the next
			// change stores it, or the next visit builds it again.
		}
	}

	/** Runs `task` after every task queued before it, whether those worked or not. */
	#inTurn<T>(task: () => Promise<T>): Promise<T> {
		const turn = this.#last.then(task, task);
		this.#last = turn.catch(() => {});
		return turn;
	}
}

/**
 * Makes a call to the core, fetching the stored files it turns out to need
 * and making the call again until it no longer asks for any.
 */
async function fetching<T>(files: StoredFiles, store: FileStore, call: () => T): Promise<T> {
	for (;;) {
		try {
			return call();
		} catch (error) {
			const wanted = files.takeWanted();
			// It failed for a reason of its own, not for want of a file.
			if (wanted.length === 0) throw error;
			for (const name of wanted) files.provide(name, await store.read(name));
		}
	}
}

/**
 * Has the core take in the stored flight files its index does not list: all
 * of them after the index was lost or its format changed, otherwise one
 * that another tab stored a moment ago.
 *
 * Each file is fetched once. One that still does not read after that is not
 * a flight and stays out of the logbook, as it does on a phone.
 */
async function takeInUnlisted(files: StoredFiles, core: Core, store: FileStore): Promise<void> {
	const fetched = new Set<string>();
	for (;;) {
		const wanted = files.takeWanted().filter((name) => !fetched.has(name));
		if (wanted.length === 0) return;
		for (const name of wanted.slice(0, FETCHED_AT_ONCE)) {
			fetched.add(name);
			files.provide(name, await store.read(name));
		}
		core.refresh();
	}
}
