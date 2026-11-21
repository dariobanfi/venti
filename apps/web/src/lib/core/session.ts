/**
 * The Rust core with a browser's store behind it.
 *
 * The core's calls do not wait, and a browser's storage always makes a page
 * wait. This is where the two meet. The core says which stored files it
 * needs and which changes it wants stored; this class does the reading and
 * the writing and repeats the call. `crates/core-wasm` describes the core's
 * side.
 *
 * Nothing here knows what a flight is. Every rule about the logbook, the
 * account and airspace is in the core.
 */

import { airspaceHeightFilter, Core, StoredFiles } from './pkg/venti_core_wasm.js';
import type {
	AirspaceFile,
	AirspaceHeightFilter,
	AirspaceImport,
	AirspaceOverPlace,
	AirspaceShape,
	Bounds,
	Coordinate,
	FlightAirspace,
	FlightAnalysis,
	FlightFile,
	LeagueHandover,
	FlightView,
	ImportedFlight,
	Credit,
	KeptSites,
	SiteView,
	LogbookStatistics,
	SiteStatistics,
	Tendencies,
	LogbookView,
	SyncStep
} from './pkg/venti_core_wasm.js';
import { type FileStore, StaleStoreError } from './store';

/**
 * How many flight files are fetched before the core takes them into a
 * logbook whose index was lost. It bounds what is in memory at once.
 */
const FETCHED_AT_ONCE = 8;

/** How often a change is tried again when another tab keeps changing the store first. */
const ATTEMPTS = 4;

/**
 * What a change does to the session token kept beside the files: a token
 * to keep from now on, `null` to forget the one kept, nothing to leave it.
 */
export type TokenChange = string | null | undefined;

/**
 * A run of the core's as the core hands it out: the logbook's sync
 * (`Core.sync`) and the one of the pilot's own sites (`Core.keptSitesSync`)
 * have the same steps.
 */
interface CoreRun {
	start(nowUnixS: number): SyncStep;
	receive(status: number, body: Uint8Array, retryAfterS?: number | null): SyncStep;
	free(): void;
}

/**
 * One run of a sync of the core's: the core says what to send to the server
 * and reads each answer, and what a step changed is stored before the next
 * one is taken. `account-source.ts` sends the requests in between.
 */
export interface SyncRun {
	/** Begins. `nowUnixS` is kept as the time of the sync. */
	start(nowUnixS: number): Promise<SyncStep>;
	/** Takes the server's answer to the request of the step before. */
	receive(status: number, body: Uint8Array, retryAfterS: number | undefined): Promise<SyncStep>;
	/** Lets go of the run, finished or not. */
	end(): void;
}

export class CoreSession {
	#store: FileStore;
	#files!: StoredFiles;
	#core!: Core;
	/** The store's revision that `#files` describes. */
	#revision = 0;
	/** How many times the core has been built from the store. */
	#generation = 0;
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

	/** The places flights began at, the one flown from most often first. */
	siteStatistics(): SiteStatistics[] {
		return this.#core.siteStatistics();
	}

	/**
	 * What the pilot's own tracks say about how they fly. An older flight
	 * whose file is not in memory is left out the first time and wanted:
	 * its file is fetched and the core asked again, until it wants none.
	 * What the core kept of those flights' figures is stored.
	 */
	tendencies(): Promise<Tendencies> {
		return this.#inTurn(async () => {
			const fetched = new Set<string>();
			for (;;) {
				const found = this.#core.tendencies();
				const wanted = this.#files.takeWanted().filter((name) => !fetched.has(name));
				if (wanted.length === 0) {
					try {
						await this.#commit();
					} catch {
						// Figures kept for later; counted again next time if not stored.
					}
					return found;
				}
				for (const name of wanted) {
					fetched.add(name);
					this.#files.provide(name, await this.#store.read(name));
				}
			}
		});
	}

	importFlight(igcText: string): Promise<ImportedFlight> {
		return this.change((core) => core.importFlight(igcText));
	}

	flight(id: string): Promise<FlightView> {
		return this.ask((core) => core.flight(id));
	}

	flightAnalysis(id: string): Promise<FlightAnalysis> {
		return this.ask((core) => core.flightAnalysis(id));
	}

	flightFile(id: string): Promise<FlightFile> {
		return this.ask((core) => core.flightFile(id));
	}

	/**
	 * What the pilot is told before a flight goes to a league, with its
	 * file: by the clock of `nowUnixS`.
	 */
	leagueHandover(id: string, nowUnixS: number): Promise<LeagueHandover> {
		return this.ask((core) => core.leagueHandover(id, nowUnixS));
	}

	deleteFlight(id: string): Promise<void> {
		return this.change((core) => core.deleteFlight(id));
	}

	/** Takes in an airspace file as the pilot picked or dropped it. */
	importAirspace(fileName: string, bytes: Uint8Array, nowUnixS: number): Promise<AirspaceImport> {
		return this.change((core) => core.importAirspace(fileName, bytes, nowUnixS));
	}

	/** The airspace files the pilot has brought in. */
	airspaceFiles(): Promise<AirspaceFile[]> {
		return this.ask((core) => core.airspaceFiles());
	}

	deleteAirspaceFile(storedName: string): Promise<void> {
		return this.change((core) => core.deleteAirspaceFile(storedName));
	}

	/**
	 * The airspaces that touch a box of the map; with a ceiling, without
	 * those that certainly begin above it.
	 */
	airspaceIn(box: Bounds, ceilingM?: number): Promise<AirspaceShape[]> {
		return this.ask((core) => core.airspaceIn(box.south, box.west, box.north, box.east, ceilingM));
	}

	/** What airspace lies over a place, from the lowest floor up, in the core's words. */
	airspaceOver(place: Coordinate): Promise<AirspaceOverPlace> {
		return this.ask((core) => core.airspaceOver(place.lat, place.lon));
	}

	/** The heights the map's airspace can be filtered by, in the core's words. */
	airspaceHeightFilter(): AirspaceHeightFilter {
		return airspaceHeightFilter();
	}

	/**
	 * One flight checked against the airspace there is now. The core asks
	 * for the flight's file and for every airspace file it has not read yet,
	 * one after another, before it answers.
	 */
	flightAirspace(id: string): Promise<FlightAirspace> {
		return this.ask((core) => core.flightAirspace(id));
	}

	/**
	 * Marks a site as one of the pilot's own and keeps its record in the
	 * browser, with its source's credit from `credits` and the date of the
	 * source's data.
	 */
	keepSite(site: SiteView, credits: Credit[], asOfUnixS: number | undefined): Promise<void> {
		return this.change((core) => core.keepSite(site, credits, asOfUnixS));
	}

	/** Unmarks a site, by its identifier. */
	forgetSite(id: string): Promise<void> {
		return this.change((core) => core.forgetSite(id));
	}

	/** Whether a site is one of the pilot's own. */
	isSiteKept(id: string): Promise<boolean> {
		return this.ask((core) => core.isSiteKept(id));
	}

	/** The pilot's own sites: from a place the nearest first, without one by name. */
	keptSites(from: Coordinate | undefined): Promise<KeptSites> {
		return this.ask((core) => core.keptSites(from));
	}

	/**
	 * Asks the core something that changes nothing, fetching the stored
	 * files the answer turns out to need.
	 */
	ask<T>(call: (core: Core) => T): Promise<T> {
		return this.#inTurn(() => fetching(this.#files, this.#store, () => call(this.#core)));
	}

	/**
	 * Makes a call that changes what is stored, and stores the change. Only
	 * when it is stored does the caller hear that it worked.
	 *
	 * `token` says, from the call's result, what becomes of the session
	 * token: it is written with the change or not at all.
	 */
	change<T>(call: (core: Core) => T, token?: (result: T) => TokenChange): Promise<T> {
		return this.#inTurn(async () => {
			for (let attempt = 1; ; attempt += 1) {
				const result = await fetching(this.#files, this.#store, () => call(this.#core));
				try {
					await this.#commit(token?.(result));
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

	/** The session token of the account this browser is signed in to, if any. */
	token(): Promise<string | undefined> {
		return this.#store.token();
	}

	/** Forgets the session token, when the server no longer takes it. */
	forgetToken(): Promise<void> {
		return this.#store.forgetToken();
	}

	/**
	 * Starts a run of the core's sync: the logbook's, or the one `of` names.
	 * Other calls take their turns between its steps, so the page stays
	 * usable while flights travel.
	 *
	 * A run belongs to the core that began it. When the store changes behind
	 * that core, in another tab or because a step could not be stored, the
	 * run's next step throws a {@link StaleStoreError}: every step before it
	 * is stored, and a new run goes on from there.
	 */
	async beginSync(of: (core: Core) => CoreRun = (core) => core.sync()): Promise<SyncRun> {
		const { run, generation } = await this.#inTurn(async () => ({
			run: of(this.#core),
			generation: this.#generation
		}));
		const step = (take: (run: CoreRun) => SyncStep) =>
			this.#inTurn(async () => {
				if (generation !== this.#generation) throw new StaleStoreError();
				const result = await fetching(this.#files, this.#store, () => take(run));
				try {
					await this.#commit();
					return result;
				} catch (error) {
					await this.#load();
					throw error;
				}
			});
		return {
			start: (nowUnixS) => step((run) => run.start(nowUnixS)),
			receive: (status, body, retryAfterS) => step((run) => run.receive(status, body, retryAfterS)),
			end: () => run.free()
		};
	}

	/**
	 * Starts a run that brings the pilot's own sites in step with their
	 * account. It is run and stored as the logbook's sync is.
	 */
	beginKeptSitesSync(): Promise<SyncRun> {
		return this.beginSync((core) => core.keptSitesSync());
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

	/** Hands the core's staged changes to the store. */
	async #commit(token?: TokenChange): Promise<void> {
		const changes = this.#files.takeStaged();
		if (changes.length > 0 || token !== undefined) {
			this.#revision = await this.#store.commit(changes, this.#revision, token);
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
		this.#generation += 1;
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
