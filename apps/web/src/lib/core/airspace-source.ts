/**
 * Airspace from the Venti server, as the worker fetches it and the core
 * keeps it.
 *
 * The core says when it is time to ask (nothing is kept, what is kept is a
 * day old, or the place is at the edge of it or beyond), what to ask for,
 * and what the answer means; it keeps the answer among its files, so the
 * maps have their airspace without the network from then on. This file
 * sends the one request in between.
 *
 * The server is the one this site is served from, as for the wind.
 */

import { type Coordinate, type KeptAirspace, serverAirspacePath } from './pkg/venti_core_wasm.js';
import { unreachable } from './protocol';
import type { CoreSession } from './session';
import { type Get, getFromThisOrigin } from './wind-source';

/** Fetches from this site's origin, and says what it was for when no answer comes. */
const getAirspace: Get = async (path) => {
	try {
		return await getFromThisOrigin(path);
	} catch {
		throw unreachable('The server could not be reached for airspace.');
	}
};

export class AirspaceSource {
	#session: CoreSession;
	#get: Get;
	/** The address the request goes to, as the core remembers the answer by it. */
	#server: string;
	#now: () => number;
	/** Whether this site has a server at all. One without is never asked. */
	#present: () => Promise<boolean>;

	constructor(
		session: CoreSession,
		present: () => Promise<boolean> = async () => true,
		get: Get = getAirspace,
		server: string = location.origin,
		now: () => number = Date.now
	) {
		this.#session = session;
		this.#present = present;
		this.#get = get;
		this.#server = server;
		this.#now = now;
	}

	/**
	 * Asks the server for the airspace around a place and keeps it, if the
	 * core says it is time. Returns what is kept afterwards, or nothing when
	 * the server was not asked. Throws, with words for the pilot, when the
	 * server gives no usable answer; what was kept before then stays.
	 */
	async refresh(place: Coordinate): Promise<KeptAirspace[] | undefined> {
		if (!(await this.#present())) return undefined;
		const now = Math.floor(this.#now() / 1000);
		const due = await this.#session.ask((core) =>
			core.keptAirspaceIsDue(this.#server, place.lat, place.lon, now)
		);
		if (!due) return undefined;
		const answer = await this.#get(serverAirspacePath(place.lat, place.lon));
		const fetched = await this.#session.change((core) =>
			core.keepServerAirspace(this.#server, place.lat, place.lon, answer.status, answer.body, now)
		);
		return fetched.kept;
	}

	/** The airspace kept from the server, one entry for each source. */
	kept(): Promise<KeptAirspace[]> {
		return this.#session.ask((core) => core.keptAirspace());
	}

	/** Removes what is kept from the server. */
	forget(): Promise<void> {
		return this.#session.change((core) => core.deleteKeptAirspace());
	}
}
