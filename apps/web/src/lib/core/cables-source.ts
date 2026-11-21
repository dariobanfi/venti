/**
 * Cables (power lines and ropeways) from the Venti server, as the worker
 * fetches them: the core says what to ask for around a place
 * (`cablesRequest`, a box of some 65 by 65 km) and reads the answer into
 * a map layer (`cablesLayer`), each cable with its words, the credits and
 * the sentence on what the layer cannot promise.
 *
 * Nothing is kept here: the browser's own cache reuses an answer for as
 * long as the server allows, and the page decides when to ask again. The
 * server is the one this site is served from, as for the wind.
 */

import {
	type CablesMap,
	cablesLayer,
	cablesRequest,
	type Coordinate
} from './pkg/venti_core_wasm.js';
import { unreachable } from './protocol';
import { type Get, getFromThisOrigin } from './wind-source';

/** Fetches from this site's origin, and says what it was for when no answer comes. */
const getCables: Get = async (path) => {
	try {
		return await getFromThisOrigin(path);
	} catch {
		throw unreachable('Venti could not be reached for the cables.');
	}
};

export class CablesSource {
	/** Whether this site has a server at all. One without is never asked. */
	#present: () => Promise<boolean>;
	#get: Get;

	constructor(present: () => Promise<boolean> = async () => true, get: Get = getCables) {
		this.#present = present;
		this.#get = get;
	}

	/**
	 * The cables around a place, as a map layer. Nothing if this site has
	 * no server. Throws, with words for the pilot, when the server gives
	 * no usable answer.
	 */
	async around(place: Coordinate): Promise<CablesMap | undefined> {
		if (!(await this.#present())) return undefined;
		const request = cablesRequest(place.lat, place.lon);
		const answer = await this.#get(request.path);
		return cablesLayer(answer.status, answer.body);
	}
}
