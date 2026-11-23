/**
 * Live wind from the Venti server, as the worker fetches it.
 *
 * The core says what to ask the server for and reads its answers. This file
 * sends the requests in between and keeps the one thing that outlives a
 * request: how far the server's clock is ahead of this device's. The age of
 * a reading is measured against the server's clock, because a device's can
 * be wrong.
 *
 * The server is the one this site is served from: the app and the API share
 * an origin. A site without a server answers the first question with "not
 * found", and the app then has no wind, as the iPhone app has none until a
 * server is set.
 */

import { unreachable } from './protocol';
import {
	type Bounds,
	type Registration,
	serverStatus,
	serverStatusPath,
	stationAir,
	type StationAir,
	stationDay,
	type StationDay,
	stationDayPath,
	stationDays,
	type StationDays,
	stationHistory,
	type StationHistory,
	stationHistoryPath,
	stationMeasuredWords,
	type WindStation,
	windMap,
	type WindMap,
	windMapPath
} from './pkg/venti_core_wasm.js';

/** What a request brought back. */
export interface Answer {
	status: number;
	body: Uint8Array;
}

/** Fetches a path from the server. Throws when no answer came at all. */
export type Get = (path: string) => Promise<Answer>;

/**
 * How far the pilot's clock is ahead of UTC at a moment, in seconds; at
 * this moment where the moment is not known (0: the server's clock not read).
 */
export function utcOffsetAt(unixS: number): number {
	return -new Date(unixS > 0 ? unixS * 1000 : Date.now()).getTimezoneOffset() * 60;
}

/** How long the server's clock is trusted before it is read again, in milliseconds. */
const CLOCK_KEPT_MS = 60_000;

/** How long a request may take before the server counts as out of reach, in milliseconds. */
const PATIENCE_MS = 20_000;

/**
 * Fetches from the origin this site is served from, with the browser's own
 * cache, which honours how long the server lets an answer be reused.
 */
export const getFromThisOrigin: Get = async (path) => {
	try {
		const response = await fetch(path, {
			headers: { accept: 'application/json' },
			signal: AbortSignal.timeout(PATIENCE_MS)
		});
		return { status: response.status, body: new Uint8Array(await response.arrayBuffer()) };
	} catch {
		// The browser says "Failed to fetch" for every cause; say what it means.
		throw unreachable('Venti could not be reached for the wind.');
	}
};

export class WindSource {
	#get: Get;
	#deviceNow: () => number;
	/** The server's clock minus this device's, and when it was read. */
	#clock: { aheadMs: number; readAt: number } | undefined;
	/** Who may create an account on the server, by its own last word. */
	#registration: Registration | undefined;
	/** Whether the server takes flying sites from pilots, by its own last word. */
	#takesSites = false;
	/** Whether the server signs pilots in with Google, by its own last word. */
	#googleSignIn = false;
	/** Whether the site turned out to have no server. */
	#absent = false;

	constructor(get: Get = getFromThisOrigin, deviceNow: () => number = Date.now) {
		this.#get = get;
		this.#deviceNow = deviceNow;
	}

	/**
	 * The stations in a box of the map. Nothing if this site has no server.
	 * Throws, with words for the pilot, when the server gives no usable answer.
	 */
	async windMap(box: Bounds): Promise<WindMap | undefined> {
		if (!(await this.#readClock())) return undefined;
		const answer = await this.#get(windMapPath(box.south, box.west, box.north, box.east));
		return windMap(answer.status, answer.body, this.#serverClock());
	}

	/**
	 * Whether this site has a Venti server behind it. A server that could
	 * not be asked counts as one: what is wrong with it is said when it is
	 * next needed.
	 */
	async present(): Promise<boolean> {
		try {
			return await this.#readClock();
		} catch {
			return true;
		}
	}

	/**
	 * Who may create an account on this site's server: anyone, whoever has
	 * an invite code, or nobody. Nothing when the server does not say, as
	 * one older than the question, when it could not be asked, and when the
	 * site has none.
	 */
	async registration(): Promise<Registration | undefined> {
		try {
			return (await this.#readClock()) ? this.#registration : undefined;
		} catch {
			return undefined;
		}
	}

	/**
	 * Whether this site's server takes flying sites from pilots: whether
	 * to offer adding and correcting one at all. No when the server does
	 * not say, when it could not be asked, and when the site has none.
	 */
	async takesSitesFromPilots(): Promise<boolean> {
		try {
			return (await this.#readClock()) && this.#takesSites;
		} catch {
			return false;
		}
	}

	/**
	 * Whether this site's server signs pilots in with Google: whether to
	 * offer Google's button at all. No when the server does not say, when
	 * it could not be asked, and when the site has none.
	 */
	async googleSignIn(): Promise<boolean> {
		try {
			return (await this.#readClock()) && this.#googleSignIn;
		} catch {
			return false;
		}
	}

	/** What a station measured over the last 24 hours. */
	async stationHistory(stationId: string): Promise<StationHistory> {
		const answer = await this.#get(stationHistoryPath(stationId));
		return stationHistory(answer.status, answer.body, this.#serverClock());
	}

	/**
	 * What a station's readings say about the air: the cloud base they
	 * give, and the change in temperature to a station much higher or
	 * lower nearby, of `stations`.
	 */
	air(station: WindStation, stations: WindStation[]): StationAir {
		return stationAir(station, stations);
	}

	/**
	 * What a station's page says under its newest reading, in the core's
	 * words: its age by the server's clock, or the time it was measured
	 * while that clock is not known.
	 */
	async measuredWords(station: WindStation, utcOffsetS: number): Promise<string> {
		return stationMeasuredWords(station, await this.serverNow(), utcOffsetS);
	}

	/** The days a station's readings can be asked for, by Venti's clock and the reader's offset from UTC. */
	async days(utcOffsetS: number): Promise<StationDays> {
		return stationDays(await this.now(), utcOffsetS);
	}

	/** What a station measured on one day, a date such as "2026-11-18". */
	async day(stationId: string, date: string, utcOffsetS: number): Promise<StationDay> {
		const answer = await this.#get(stationDayPath(stationId, date, utcOffsetS));
		return stationDay(answer.status, answer.body, date, utcOffsetS);
	}

	/**
	 * The current time by Venti's clock, in seconds, read with the health
	 * check; this device's where there is no server to ask.
	 */
	async now(): Promise<number> {
		await this.present();
		return this.#serverNow();
	}

	/**
	 * The current time by the server's clock alone, in seconds, moved on by
	 * the time passed since it was read: 0 while the app has not read it (no
	 * server, or one that did not answer), never this device's own. Ages a
	 * pilot reads of another pilot ("3 h ago") are counted from it; for 0
	 * the core says the day instead of an age.
	 */
	async serverNow(): Promise<number> {
		await this.present();
		return this.#serverClock();
	}

	/** The server's clock as it stands, or 0 while it is not known: the core then counts no age. */
	#serverClock(): number {
		return this.#clock ? this.#serverNow() : 0;
	}

	/** The current time by the server's clock, in seconds. */
	#serverNow(): number {
		return Math.round((this.#deviceNow() + (this.#clock?.aheadMs ?? 0)) / 1000);
	}

	/**
	 * Asks the server how it is doing, unless it was asked a moment ago.
	 * Says whether there is a server at all.
	 */
	async #readClock(): Promise<boolean> {
		if (this.#absent) return false;
		if (this.#clock && this.#deviceNow() - this.#clock.readAt < CLOCK_KEPT_MS) return true;

		const answer = await this.#get(serverStatusPath());
		// A site that never answered as a server and does not know the path
		// has none. One that did answer before is a server in trouble, and
		// the core puts that in words.
		if (answer.status === 404 && !this.#clock) {
			this.#absent = true;
			return false;
		}
		const status = serverStatus(answer.status, answer.body);
		const now = this.#deviceNow();
		this.#clock = { aheadMs: status.timeUnixS * 1000 - now, readAt: now };
		this.#registration = status.registration;
		this.#takesSites = status.takesSitesFromPilots ?? false;
		this.#googleSignIn = status.googleSignIn ?? false;
		return true;
	}
}
