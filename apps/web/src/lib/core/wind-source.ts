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

import { UNREACHABLE } from './protocol';
import {
	type Bounds,
	serverStatus,
	serverStatusPath,
	stationHistory,
	type StationHistory,
	stationHistoryPath,
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

/** The wind map of a box, with what the server said about its providers. */
export interface WindAnswer {
	map: WindMap;
	/** The providers the server has not heard from lately. */
	lateProviders: string[];
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
		const error = new Error('The wind server could not be reached.');
		error.name = UNREACHABLE;
		throw error;
	}
};

export class WindSource {
	#get: Get;
	#deviceNow: () => number;
	/** The server's clock minus this device's, and when it was read. */
	#clock: { aheadMs: number; readAt: number } | undefined;
	#lateProviders: string[] = [];
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
	async windMap(box: Bounds): Promise<WindAnswer | undefined> {
		if (!(await this.#readClock())) return undefined;
		const answer = await this.#get(windMapPath(box.south, box.west, box.north, box.east));
		return {
			map: windMap(answer.status, answer.body, this.#serverNow()),
			lateProviders: this.#lateProviders
		};
	}

	/** What a station measured over the last 24 hours. */
	async stationHistory(stationId: string): Promise<StationHistory> {
		const answer = await this.#get(stationHistoryPath(stationId));
		return stationHistory(answer.status, answer.body, this.#serverNow());
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
		this.#lateProviders = status.lateProviders;
		return true;
	}
}
