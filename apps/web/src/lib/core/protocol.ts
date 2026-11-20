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
import type { AirspaceSource } from './airspace-source';
import type { CommunitySource, OpenedFlight, PublicFlightsAsked } from './community-source';
import { flightFileText } from './flight-text';
import type {
	AccountView,
	AirspaceFile,
	AirspaceImport,
	AirspaceShape,
	Bounds,
	Coordinate,
	FlightAirspace,
	FlightAnalysis,
	FlightFile,
	FlightSharing,
	FlightView,
	FlightVisibility,
	ForecastView,
	GroundUnderFlight,
	ImportedFlight,
	KeptAirspace,
	LogbookStatistics,
	LogbookView,
	PilotPage,
	PublicFlights,
	Registration,
	ReportForm,
	ReportReason,
	ReportSubject,
	SitesMap,
	StationHistory
} from './pkg/venti_core_wasm.js';
import type { ForecastSource } from './forecast-source';
import type { LiveAnswer, LiveFollowAnswer, LiveSource } from './live-source';
import type { CoreSession } from './session';
import type { SitesSource } from './sites-source';
import type { GroundAsked, TerrainSource } from './terrain-source';
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
	/**
	 * The ground under a flight's track, read from the map's elevation
	 * tiles, which are fetched for it. Empty where there is none to be
	 * had, and when the page has asked about another flight by now.
	 */
	flightGround: { given: GroundAsked; answer: GroundUnderFlight[] };
	/** Given the number of a question about the ground: lets go of it. */
	forgetFlightGround: { given: number; answer: undefined };
	/** The wind stations in a box of the map. Nothing if this site has no server. */
	wind: { given: Bounds; answer: WindAnswer | undefined };
	/** Given a station's identifier: what it measured over the last 24 hours. */
	stationHistory: { given: string; answer: StationHistory };
	/** The flying sites in a box of the map. Nothing if this site has no server. */
	sites: { given: Bounds; answer: SitesMap | undefined };
	/** What the server's weather models say about a place. Nothing if this site has no server. */
	forecast: { given: Coordinate; answer: ForecastView | undefined };
	/**
	 * The gliders in a box of the map whose pilots show their flight to
	 * everyone. Nothing if this site has no server.
	 */
	live: { given: Bounds; answer: LiveAnswer | undefined };
	/**
	 * Given a live track's identifier: asks the server once about that
	 * flight, for all of it the first time and then for what is new.
	 * Nothing if this site has no server, or the page follows another
	 * flight by now.
	 */
	followLive: { given: string; answer: LiveFollowAnswer | undefined };
	/** Lets go of the flight being followed. */
	unfollowLive: { given: undefined; answer: undefined };
	/** How old positions are, in the core's words, one answer for each. */
	liveAges: { given: { ageS: number; landed: boolean }[]; answer: string[] };
	/**
	 * One page of the flights the server's pilots share with everyone.
	 * Nothing if this site has no server.
	 */
	publicFlights: { given: PublicFlightsAsked; answer: PublicFlights | undefined };
	/** A pilot, by handle, and one page of that pilot's public flights. */
	pilotPage: { given: { handle: string; after?: string }; answer: PilotPage | undefined };
	/**
	 * Given a flight's address on the server: another pilot's flight,
	 * fetched and prepared for display. Nothing of it is kept.
	 */
	openPublicFlight: { given: string; answer: OpenedFlight | undefined };
	/** Whether this site has a Venti server behind it. Yes, if that cannot be found out. */
	hasServer: { given: undefined; answer: boolean };
	/** Who may create an account on this site's server, if the server says. */
	registration: { given: undefined; answer: Registration | undefined };
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
	/** What a report can say, and what the pilot is told before sending one. */
	reportForm: { given: undefined; answer: ReportForm };
	/**
	 * Reports a flight, a pilot's page or a live track to whoever runs the
	 * server, from the pilot's account. The answer is what became of it, in
	 * a line.
	 */
	report: {
		given: { subject: ReportSubject; reason: ReportReason; text: string };
		answer: string;
	};
	/** An airspace file's name and bytes, as the pilot picked or dropped it. */
	importAirspace: { given: { name: string; bytes: ArrayBuffer }; answer: AirspaceImport };
	airspaceFiles: { given: undefined; answer: AirspaceFile[] };
	/** Given the name a file is stored under. */
	deleteAirspaceFile: { given: string; answer: undefined };
	/** The airspaces that touch a box of the map, from the server's and from the pilot's files. */
	airspaceIn: { given: Bounds; answer: AirspaceShape[] };
	/** Given a flight's identifier: the flight checked against the airspace there is now. */
	flightAirspace: { given: string; answer: FlightAirspace };
	/**
	 * Given a place: asks the server for the airspace around it and keeps it,
	 * if the core says it is time. What is kept afterwards, or nothing when
	 * the server was not asked.
	 */
	refreshAirspace: { given: Coordinate; answer: KeptAirspace[] | undefined };
	/** The airspace kept from the server, one entry for each source. */
	keptAirspace: { given: undefined; answer: KeptAirspace[] };
	forgetKeptAirspace: { given: undefined; answer: undefined };
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
	airspace: AirspaceSource;
	sites: SitesSource;
	forecast: ForecastSource;
	live: LiveSource;
	community: CommunitySource;
	terrain: TerrainSource;
}

/** Does what a call asks of the core. */
export async function answer<Name extends CallName>(
	{ session, wind, account, airspace, sites, forecast, live, community, terrain }: Answering,
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
		flightGround: (asked) => terrain.under(asked),
		forgetFlightGround: (asking) => terrain.forget(asking),
		wind: (box) => wind.windMap(box),
		stationHistory: (id) => wind.stationHistory(id),
		sites: (box) => sites.sites(box),
		forecast: (place) => forecast.forecast(place),
		live: (box) => live.gliders(box),
		followLive: (id) => live.follow(id),
		unfollowLive: () => live.unfollow(),
		liveAges: (asked) => live.ages(asked),
		publicFlights: (asked) => community.flights(asked),
		pilotPage: ({ handle, after }) => community.pilot(handle, after),
		openPublicFlight: (id) => community.open(id),
		hasServer: () => wind.present(),
		registration: () => wind.registration(),
		account: () => account.account(),
		signIn: ({ handle, password }) => account.signIn(handle, password),
		createAccount: (wanted) => account.createAccount(wanted),
		signOut: () => account.signOut(),
		deleteAccount: (password) => account.deleteAccount(password),
		sync: () => account.sync(),
		flightSharing: (id) => account.sharing(id),
		shareFlight: ({ id, visibility }) => account.share(id, visibility),
		reportForm: () => account.reportForm(),
		report: ({ subject, reason, text }) => account.report(subject, reason, text),
		importAirspace: ({ name, bytes }) =>
			session.importAirspace(name, new Uint8Array(bytes), Math.floor(Date.now() / 1000)),
		airspaceFiles: () => session.airspaceFiles(),
		deleteAirspaceFile: (storedName) => session.deleteAirspaceFile(storedName),
		airspaceIn: (box) => session.airspaceIn(box),
		flightAirspace: (id) => session.flightAirspace(id),
		refreshAirspace: (place) => airspace.refresh(place),
		keptAirspace: () => airspace.kept(),
		forgetKeptAirspace: () => airspace.forget()
	};
	return (await answers[call](given)) as Calls[Name]['answer'];
}

/**
 * The name of the error for a server that gave no answer at all. Like the
 * core's own errors, its message is written for the pilot.
 */
export const UNREACHABLE = 'Unreachable';

/** The error for a server that gave no answer at all, with words for the pilot. */
export function unreachable(message: string): Error {
	const error = new Error(message);
	error.name = UNREACHABLE;
	return error;
}

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
