/**
 * The pilot's own flights as the screens see them: the account's, on the
 * Venti server. The website keeps no logbook of its own (owner decision 20):
 * a pilot signs in to see or import flights, and importing a file is an
 * upload to the account.
 *
 * The model holds the pages read so far, newest first, for the screens to
 * draw; the worker asks the server and the core reads its answers
 * (`core/own-flights-source.ts`). It contains no flying logic.
 */

import {
	type CoreClient,
	type FlightAirspace,
	type FlightVisibility,
	type GliderClassKind,
	type LeagueHandover,
	type PilotStatistics,
	type Tendencies,
	isCoreError,
	isInWords,
	type OpenedOwnFlight,
	type OwnFlight
} from '#lib/core/index.ts';
import { emptyReport, type ImportReport } from './import-report';

/** Where the list stands. */
export type MyFlightsPhase =
	{ is: 'loading' } | { is: 'signedOut' } | { is: 'loaded' } | { is: 'failed'; message: string };

export class MyFlightsModel {
	/** The flights read so far, the newest first. */
	flights = $state.raw<OwnFlight[]>([]);

	/** What to ask for the page after the last one read. Nothing when every page is read. */
	next = $state.raw<string>();

	phase = $state.raw<MyFlightsPhase>({ is: 'loading' });

	/** How far a running upload has got, for the screen to say. */
	progress = $state.raw<{ done: number; of: number }>();

	/** Whether a further page is on its way. */
	loadingMore = $state(false);

	/** Counts the changes to the list, for screens that read it anew. */
	revision = $state(0);

	#core: CoreClient;
	/** Only the answer to the latest question counts. */
	#asked = 0;

	constructor(core: CoreClient) {
		this.#core = core;
	}

	/** The version of the core, for bug reports. */
	get coreVersion(): string {
		return this.#core.version;
	}

	/** Every flight's outline read so far, for the map's layer of the pilot's flights. */
	get outlines() {
		return this.flights.map((flight) => flight.row.outline);
	}

	/**
	 * Reads the first page again: when the app starts, after signing in or
	 * out, and after a change made here.
	 */
	async reload(): Promise<void> {
		const asked = (this.#asked += 1);
		if (this.phase.is !== 'loaded') this.phase = { is: 'loading' };
		try {
			const page = await this.#core.ownFlights();
			if (asked !== this.#asked) return;
			if (!page) {
				this.flights = [];
				this.next = undefined;
				this.phase = { is: 'signedOut' };
			} else {
				this.flights = page.flights;
				this.next = page.next;
				this.phase = { is: 'loaded' };
			}
			this.revision += 1;
		} catch (error) {
			if (asked === this.#asked) this.phase = { is: 'failed', message: reasonFor(error) };
		}
	}

	/** Reads the next page, if there is one. */
	async more(): Promise<void> {
		if (!this.next || this.loadingMore) return;
		const asked = this.#asked;
		this.loadingMore = true;
		try {
			const page = await this.#core.ownFlights(this.next);
			if (asked !== this.#asked || !page) return;
			this.flights = [...this.flights, ...page.flights];
			this.next = page.next;
		} catch (error) {
			this.phase = { is: 'failed', message: reasonFor(error) };
		} finally {
			this.loadingMore = false;
		}
	}

	/** Uploads flight files to the account, one after another, and reports on each. */
	async upload(files: readonly File[]): Promise<ImportReport> {
		const report = emptyReport();
		this.progress = { done: 0, of: files.length };
		try {
			for (const [index, file] of files.entries()) {
				try {
					const uploaded = await this.#core.uploadFlight(await file.arrayBuffer());
					if (!uploaded) {
						report.failures.push({ file: file.name, reason: SIGN_IN });
						continue;
					}
					(uploaded.alreadyThere ? report.alreadyThere : report.added).push(uploaded.flight.row.id);
				} catch (error) {
					report.failures.push({ file: file.name, reason: reasonFor(error) });
					if (!isCoreError(error)) {
						// Venti could not be reached, and will not be for the next one either.
						for (const left of files.slice(index + 1)) {
							report.failures.push({ file: left.name, reason: reasonFor(error) });
						}
						break;
					}
				}
				this.progress = { done: index + 1, of: files.length };
			}
		} finally {
			await this.reload();
			this.progress = undefined;
		}
		return report;
	}

	/** One flight, prepared for display. */
	async open(id: string): Promise<OpenedOwnFlight> {
		const opened = await this.#core.openOwnFlight(id);
		if (!opened) throw new Error(SIGN_IN);
		return opened;
	}

	/** What all the pilot's flights add up to, the private ones too, as the core reads it. */
	async statistics(): Promise<PilotStatistics> {
		const statistics = await this.#core.ownStatistics();
		if (!statistics) throw new Error(SIGN_IN);
		return statistics;
	}

	/** What the pilot's flights on the account say about how they fly ("How you fly"). */
	async tendencies(): Promise<Tendencies> {
		const tendencies = await this.#core.ownTendencies();
		if (!tendencies) throw new Error(SIGN_IN);
		return tendencies;
	}

	/** One of the pilot's flights checked against the airspace the app has, from its file's text. */
	airspace(text: string): Promise<FlightAirspace> {
		return this.#core.ownFlightAirspace(text);
	}

	/** What the pilot is told before a flight goes to a league, from its file's text. */
	leagueHandover(text: string): Promise<LeagueHandover> {
		return this.#core.leagueHandover(text);
	}

	/** Changes who may see a flight; the list follows. */
	async share(id: string, visibility: FlightVisibility) {
		const sharing = await this.#core.shareFlight(id, visibility);
		this.flights = this.flights.map((flight) =>
			flight.row.id === id ? { ...flight, sharing } : flight
		);
		return sharing;
	}

	/**
	 * Sets the class of the glider a flight was flown on, or says it is not
	 * known: for a flight brought in before the pilot set a class. The list
	 * follows; the flight as the server now has it.
	 */
	async setGliderClass(id: string, gliderClass: GliderClassKind | undefined): Promise<OwnFlight> {
		const changed = await this.#core.setFlightGliderClass(id, gliderClass);
		this.flights = this.flights.map((flight) => (flight.row.id === id ? changed : flight));
		this.revision += 1;
		return changed;
	}

	/** Deletes a flight from the account, at once and for good. */
	async delete(id: string): Promise<void> {
		try {
			await this.#core.deleteOwnFlight(id);
		} finally {
			await this.reload();
		}
	}
}

/** What a pilot who is not signed in is told. */
export const SIGN_IN = 'Sign in to see and bring in your flights.';

function message(error: unknown): string {
	return error instanceof Error ? error.message : String(error);
}

/** Why something could not be done to a flight, in words for the pilot. */
export function reasonFor(error: unknown): string {
	if (isInWords(error)) return message(error);
	if (error instanceof Error && error.name === 'QuotaExceededError') {
		return 'This browser has no room left for this site.';
	}
	return `This did not work. ${message(error)}`;
}
