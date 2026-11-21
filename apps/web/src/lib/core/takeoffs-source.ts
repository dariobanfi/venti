/**
 * What public flights say about where pilots take off, as the worker asks
 * the Venti server about it: the flights that began at a site, the places
 * where several pilots took off and no site stands (candidates), and the
 * offer to add such a place to a pilot who took off there.
 *
 * Everything about it is the core's (`crates/core/src/takeoffs.rs`): the
 * paths, the reading of each answer with its words, the refusal of a part
 * of the map too large to ask about, who is offered what and the draft of
 * the place. This file sends the requests in between. The offer reads the
 * logbook's flights and the places set aside in this browser, and setting
 * one aside writes, so those two go through the session.
 *
 * The server is the one this site is served from, as for the sites. No
 * token: what public flights say is open.
 */

import {
	type CandidateOffer,
	type Coordinate,
	siteCandidates,
	siteCandidatesPath,
	type SiteCandidates,
	siteFlights,
	siteFlightsPath,
	type SiteFlights,
	siteFlightsWords,
	type SiteFlightsWords
} from './pkg/venti_core_wasm.js';
import { unreachable } from './protocol';
import type { CoreSession } from './session';
import { type Get, getFromThisOrigin } from './wind-source';

/** Fetches from this site's origin, and says what it was for when no answer comes. */
const getFlights: Get = async (path) => {
	try {
		return await getFromThisOrigin(path);
	} catch {
		throw unreachable('Venti could not be reached for public flights.');
	}
};

/** A part of the map, as the core asks about one. */
export interface Box {
	south: number;
	west: number;
	north: number;
	east: number;
}

export class TakeoffsSource {
	#session: CoreSession;
	#get: Get;
	/** Whether the server takes places from pilots, as its status says. */
	#takes: () => Promise<boolean>;

	constructor(
		session: CoreSession,
		takes: () => Promise<boolean> = async () => false,
		get: Get = getFlights
	) {
		this.#session = session;
		this.#takes = takes;
		this.#get = get;
	}

	/** The words around it all, the core's. Asks no server. */
	words(): SiteFlightsWords {
		return siteFlightsWords();
	}

	/**
	 * What the public flights that began at a site say about it. Throws in
	 * the core's words: no such site, removed, or asked too often.
	 */
	async site(id: string): Promise<SiteFlights> {
		const answer = await this.#get(siteFlightsPath(id));
		return siteFlights(answer.status, answer.body);
	}

	/**
	 * The places in a part of the map where pilots take off and no site
	 * stands. A part too large is refused before the server is asked, in
	 * the core's words to show over the map.
	 */
	async candidates(box: Box): Promise<SiteCandidates> {
		const path = siteCandidatesPath(box.south, box.west, box.north, box.east);
		const answer = await this.#get(path);
		return siteCandidates(answer.status, answer.body);
	}

	/**
	 * The candidates of an answer the pilot of this browser took off at,
	 * each with the core's words and a draft at the pilot's own take-off.
	 * `utcOffsetS` is how far the browser's clock is ahead of UTC.
	 */
	async offers(candidates: SiteCandidates, utcOffsetS: number): Promise<CandidateOffer[]> {
		const takes = await this.#takes();
		return this.#session.ask((core) => core.candidateForPilot(candidates, takes, utcOffsetS));
	}

	/** "Not this one": the place is not offered again in this browser. */
	setAside(position: Coordinate): Promise<void> {
		return this.#session.change((core) => core.candidateSetAside(position));
	}
}
