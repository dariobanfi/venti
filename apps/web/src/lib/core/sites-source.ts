/**
 * Flying sites from the Venti server, as the worker fetches them: take-offs,
 * landings, winch sites and training slopes for the map.
 *
 * The core says what to ask the server for and reads its answer. This file
 * sends the request in between. Nothing is kept: sites are for planning,
 * with a network, and the browser's own cache reuses an answer for as long
 * as the server allows.
 *
 * The server is the one this site is served from, as for the wind.
 */

import { type Bounds, sitesMap, type SitesMap, sitesPath } from './pkg/venti_core_wasm.js';
import { unreachable } from './protocol';
import { type Get, getFromThisOrigin } from './wind-source';

/** Fetches from this site's origin, and says what it was for when no answer comes. */
const getSites: Get = async (path) => {
	try {
		return await getFromThisOrigin(path);
	} catch {
		throw unreachable('The server could not be reached for flying sites.');
	}
};

export class SitesSource {
	/** Whether this site has a server at all. One without is never asked. */
	#present: () => Promise<boolean>;
	#get: Get;

	constructor(present: () => Promise<boolean> = async () => true, get: Get = getSites) {
		this.#present = present;
		this.#get = get;
	}

	/**
	 * The flying sites in a box of the map. Nothing if this site has no
	 * server. Throws, with words for the pilot, when the server gives no
	 * usable answer.
	 */
	async sites(box: Bounds): Promise<SitesMap | undefined> {
		if (!(await this.#present())) return undefined;
		const answer = await this.#get(sitesPath(box.south, box.west, box.north, box.east));
		// The core is told the box again: it says whether the server's set
		// is whole for all of it.
		return sitesMap(box.south, box.west, box.north, box.east, answer.status, answer.body);
	}
}
