/**
 * What many gliders say together, from the Venti server as the worker
 * fetches it: where pilots climb right now (`liveClimbs`), how many are in
 * the air at a site (`siteAirWords`), and where the public flights'
 * thermals have been (`flownThermals`). Places in the air and numbers; no
 * pilot is named. The core says what to ask for and reads every answer
 * into words.
 *
 * Nothing is kept here: the page decides when to ask again. The server is
 * the one this site is served from, as for the wind.
 */

import {
	type Bounds,
	type FlownThermals,
	flownThermals,
	flownThermalsPath,
	flownTooWideWords,
	type LiveClimbs,
	liveClimbs,
	liveClimbsEmptyWords,
	liveClimbsPath,
	liveSitesPath,
	type MapLayersWords,
	mapLayersWords,
	siteAirWords
} from './pkg/venti_core_wasm.js';
import { unreachable } from './protocol';
import { type Get, getFromThisOrigin } from './wind-source';

/** Fetches from this site's origin, and says what it was for when no answer comes. */
const getAir: Get = async (path) => {
	try {
		return await getFromThisOrigin(path);
	} catch {
		throw unreachable('Venti could not be reached for who is in the air.');
	}
};

/** The climbs of a part of the map, with what the core says where there are none. */
export type ClimbsAnswer = LiveClimbs & { emptyWords: string };

/** Where thermals have been, or the core's words for a map that shows too much to ask. */
export interface FlownAnswer {
	thermals?: FlownThermals;
	tooWide?: string;
}

/** A site as its line about the air is asked for. */
export interface SiteAirAsked {
	id: string;
	name: string;
	lat: number;
	lon: number;
}

/** How far around a site the server is asked for the sites with pilots in the air, in degrees. */
const AROUND_SITE_DEG = 0.05;

export class AirSource {
	/** Whether this site has a server at all. One without is never asked. */
	#present: () => Promise<boolean>;
	#get: Get;

	constructor(present: () => Promise<boolean> = async () => true, get: Get = getAir) {
		this.#present = present;
		this.#get = get;
	}

	/** What the layers' sheet of the map says: its title, headings, the two layers and their lines. */
	words(): MapLayersWords {
		return mapLayersWords();
	}

	/**
	 * The climbs of the last minutes in a part of the map. Nothing from a
	 * site without a server; for a part wider than the server answers for,
	 * no climbs and the core's words about zooming in.
	 */
	async climbs(box: Bounds): Promise<ClimbsAnswer | undefined> {
		if (!(await this.#present())) return undefined;
		const path = liveClimbsPath(box.south, box.west, box.north, box.east);
		if (!path) {
			return {
				climbs: [],
				credits: [],
				truncated: false,
				listedMinutes: 0,
				countWords: '',
				emptyWords: mapLayersWords().climbsTooWide
			};
		}
		const answer = await this.#get(path);
		return { ...liveClimbs(answer.status, answer.body), emptyWords: liveClimbsEmptyWords() };
	}

	/**
	 * Where thermals have been in a part of the map, in every month of the
	 * year and at every hour, so that the layer is not empty in winter; the
	 * core's words instead where the map shows too much.
	 */
	async flown(box: Bounds): Promise<FlownAnswer | undefined> {
		if (!(await this.#present())) return undefined;
		const path = flownThermalsPath(
			box.south,
			box.west,
			box.north,
			box.east,
			undefined,
			undefined,
			undefined
		);
		if (!path) return { tooWide: flownTooWideWords() };
		const answer = await this.#get(path);
		return { thermals: flownThermals(answer.status, answer.body) };
	}

	/**
	 * The core's line of who flies at a site now: "5 pilots in the air at
	 * Wallberg, 2 took off in the last hour". Empty when nobody does, and
	 * from a site without a server.
	 */
	async siteAir(site: SiteAirAsked): Promise<string> {
		if (!(await this.#present())) return '';
		const path = liveSitesPath(
			site.lat - AROUND_SITE_DEG,
			site.lon - AROUND_SITE_DEG,
			site.lat + AROUND_SITE_DEG,
			site.lon + AROUND_SITE_DEG
		);
		if (!path) return '';
		const answer = await this.#get(path);
		return siteAirWords(answer.status, answer.body, site.id, site.name);
	}
}
