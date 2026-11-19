/**
 * What the page and the core say to each other.
 *
 * The core runs in a worker, so that reading a long flight never holds up
 * the page: an eight-hour flight takes about a second to import and another
 * to analyse. The page sends a call and gets an answer; this file is the
 * list of calls, and the one place that turns a call into the core doing
 * something.
 */

import type { AccountSource, NewAccount, SyncOutcome } from './account-source';
import { flightFileText } from './flight-text';
import type {
	AccountView,
	AirspaceFile,
	AirspaceImport,
	AirspaceShape,
	Bounds,
	FlightAnalysis,
	FlightFile,
	FlightSharing,
	FlightView,
	FlightVisibility,
	ImportedFlight,
	LogbookStatistics,
	LogbookView,
	StationHistory
} from './pkg/venti_core_wasm.js';
import type { CoreSession } from './session';
import type { WindAnswer, WindSource } from './wind-source';

/** What the page learns when the core has started. */
export interface Started {
	/** Whether the logbook is still there after the page is closed. */
	durable: boolean;
	/** The version of the core, for bug reports. */
	version: string;
}

/** Every call, with what it is given and what it answers. */
export interface Calls {
	start: { given: undefined; answer: Started };
	logbook: { given: undefined; answer: LogbookView };
	statistics: { given: undefined; answer: LogbookStatistics };
	/** A flight file's bytes, as the pilot picked or dropped it. */
	importFlight: { given: ArrayBuffer; answer: ImportedFlight };
	/** The following are given a flight's identifier. */
	flight: { given: string; answer: FlightView };
	flightAnalysis: { given: string; answer: FlightAnalysis };
	flightFile: { given: string; answer: FlightFile };
	deleteFlight: { given: string; answer: undefined };
	/** The wind stations in a box of the map. Nothing if this site has no server. */
	wind: { given: Bounds; answer: WindAnswer | undefined };
	/** Given a station's identifier: what it measured over the last 24 hours. */
	stationHistory: { given: string; answer: StationHistory };
	/** Whether this site has a Venti server behind it. Yes, if that cannot be found out. */
	hasServer: { given: undefined; answer: boolean };
	/** The account this browser is signed in to, if any. */
	account: { given: undefined; answer: AccountView | undefined };
	signIn: { given: { handle: string; password: string }; answer: AccountView };
	createAccount: { given: NewAccount; answer: AccountView };
	signOut: { given: undefined; answer: undefined };
	/** Given the password: deletes the account on the server. */
	deleteAccount: { given: string; answer: undefined };
	/** Brings the logbook and the server in step. */
	sync: { given: undefined; answer: SyncOutcome };
	/** Given a flight's identifier: how it is shared, if the server has it. */
	flightSharing: { given: string; answer: FlightSharing | undefined };
	shareFlight: { given: { id: string; visibility: FlightVisibility }; answer: FlightSharing };
	/** An airspace file's name and bytes, as the pilot picked or dropped it. */
	importAirspace: { given: { name: string; bytes: ArrayBuffer }; answer: AirspaceImport };
	airspaceFiles: { given: undefined; answer: AirspaceFile[] };
	/** Given the name a file is stored under. */
	deleteAirspaceFile: { given: string; answer: undefined };
	/** The airspaces that touch a box of the map. */
	airspaceIn: { given: Bounds; answer: AirspaceShape[] };
}

export type CallName = keyof Calls;

/** A call on its way to the core. `id` pairs the answer with it. */
export interface Request<Name extends CallName = CallName> {
	id: number;
	call: Name;
	given: Calls[Name]['given'];
}

/** What comes back from the core. */
export type Reply =
	| { id: number; answer: unknown }
	/** The call failed. An error does not survive the trip whole; its name and message do. */
	| { id: number; failure: { name: string; message: string } }
	/** Not an answer: another tab changed the logbook, and the core has caught up. */
	| { changedElsewhere: true };

/** What answers the calls: the core over the browser's store, and the ways to the server. */
export interface Answering {
	session: CoreSession;
	wind: WindSource;
	account: AccountSource;
}

/** Does what a call asks of the core. */
export async function answer<Name extends CallName>(
	{ session, wind, account }: Answering,
	call: Name,
	given: Calls[Name]['given']
): Promise<Calls[Name]['answer']> {
	const answers: { [N in CallName]: (given: Calls[N]['given']) => unknown } = {
		start: () => ({ durable: session.durable, version: session.version() }),
		logbook: () => session.logbook(),
		statistics: () => session.statistics(),
		importFlight: (file) => session.importFlight(flightFileText(file)),
		flight: (id) => session.flight(id),
		flightAnalysis: (id) => session.flightAnalysis(id),
		flightFile: (id) => session.flightFile(id),
		deleteFlight: (id) => session.deleteFlight(id),
		wind: (box) => wind.windMap(box),
		stationHistory: (id) => wind.stationHistory(id),
		hasServer: () => wind.present(),
		account: () => account.account(),
		signIn: ({ handle, password }) => account.signIn(handle, password),
		createAccount: (wanted) => account.createAccount(wanted),
		signOut: () => account.signOut(),
		deleteAccount: (password) => account.deleteAccount(password),
		sync: () => account.sync(),
		flightSharing: (id) => account.sharing(id),
		shareFlight: ({ id, visibility }) => account.share(id, visibility),
		importAirspace: ({ name, bytes }) =>
			session.importAirspace(name, new Uint8Array(bytes), Math.floor(Date.now() / 1000)),
		airspaceFiles: () => session.airspaceFiles(),
		deleteAirspaceFile: (storedName) => session.deleteAirspaceFile(storedName),
		airspaceIn: (box) => session.airspaceIn(box)
	};
	return (await answers[call](given)) as Calls[Name]['answer'];
}

/**
 * The name of the error for a server that gave no answer at all. Like the
 * core's own errors, its message is written for the pilot.
 */
export const UNREACHABLE = 'Unreachable';

/** An error as it is sent to the page. */
export function describeFailure(error: unknown): { name: string; message: string } {
	return error instanceof Error
		? { name: error.name, message: error.message }
		: { name: 'Error', message: String(error) };
}

/** The error the page throws for a failure the core sent. */
export function failureAsError(failure: { name: string; message: string }): Error {
	const error = new Error(failure.message);
	error.name = failure.name;
	return error;
}
