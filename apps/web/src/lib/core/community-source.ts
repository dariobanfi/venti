/**
 * Other pilots' flights from the Venti server, as the worker fetches them:
 * the flights the server's pilots share with everyone, in one list; one
 * pilot's page of them; the ranking of the pilots by their best flights;
 * and one flight, shown from its file.
 *
 * The core says what to ask the server for and reads its answers. This file
 * sends the requests in between. Nothing is kept: a flight of another pilot
 * is fetched when it is looked at and is gone when the page is. The
 * browser's own cache may keep an answer, and checks it with the server
 * before every reuse, as the server asks; that is how a flight its pilot
 * takes back leaves the list at once.
 *
 * The server is the one this site is served from, as for the wind.
 */

import { flightFileText } from './flight-text';
import {
	type Coordinate,
	flightAnalysisFromFile,
	flownFromHere,
	type FlightAnalysis,
	flightFromFile,
	type FlightView,
	pilotPage,
	type PilotPage,
	pilotPagePath,
	publicFlight,
	type PublicFlight,
	publicFlightPath,
	publicFlights,
	type PublicFlights,
	type PublicFlightsOrder,
	publicFlightsPath,
	ranking,
	type Ranking,
	rankingPath,
	type RankingSeason,
	rankingSeasons
} from './pkg/venti_core_wasm.js';
import { unreachable } from './protocol';
import type { Get } from './wind-source';

/** How long a request may take before the server counts as out of reach, in milliseconds. */
const PATIENCE_MS = 30_000;

/**
 * Fetches from the origin this site is served from. Nothing is said about
 * what is accepted: a flight's file is no JSON.
 */
const getFromThisOrigin: Get = async (path) => {
	try {
		const response = await fetch(path, { signal: AbortSignal.timeout(PATIENCE_MS) });
		return { status: response.status, body: new Uint8Array(await response.arrayBuffer()) };
	} catch {
		throw unreachable('The server could not be reached for other pilots’ flights.');
	}
};

/** Which of everyone's flights a page asks for. */
export interface PublicFlightsAsked {
	order: PublicFlightsOrder;
	/** Only flights that began at this moment or later, seconds since 1970-01-01 UTC. */
	fromUnixS?: number;
	/** Only flights that began before this moment. */
	toUnixS?: number;
	/** The `next` of the page before, for the page after it. */
	after?: string;
	/**
	 * Only flights that took off near this place: within a kilometre and a
	 * half, as the core counts a flight flown from there.
	 */
	near?: Coordinate;
}

/** The pilots of a season ranked, with the seasons there are to choose from. */
export interface SeasonRanking {
	/** The seasons a page offers, the newest first, and all years together last. */
	seasons: RankingSeason[];
	/** The season this is the ranking of. */
	season: RankingSeason;
	ranking: Ranking;
}

/** Another pilot's flight, prepared for display like one of the logbook. */
export interface OpenedFlight {
	/** What the list says about it: whose it is, and where its page on the server is. */
	row: PublicFlight;
	flight: FlightView;
	/** What happened in it. Without it the flight is still shown in full. */
	analysis: FlightAnalysis | undefined;
}

export class CommunitySource {
	/** Whether this site has a server at all. One without is never asked. */
	#present: () => Promise<boolean>;
	#get: Get;

	constructor(present: () => Promise<boolean> = async () => true, get: Get = getFromThisOrigin) {
		this.#present = present;
		this.#get = get;
	}

	/**
	 * One page of everyone's public flights. Nothing if this site has no
	 * server. Throws, with words for the pilot, when the server gives no
	 * usable answer.
	 */
	async flights(asked: PublicFlightsAsked): Promise<PublicFlights | undefined> {
		if (!(await this.#present())) return undefined;
		const area = asked.near ? flownFromHere(asked.near.lat, asked.near.lon) : undefined;
		const answer = await this.#get(
			publicFlightsPath(asked.order, area, asked.fromUnixS, asked.toUnixS, asked.after)
		);
		return publicFlights(answer.status, answer.body);
	}

	/**
	 * A pilot and one page of the flights that pilot shares with everyone.
	 * Nothing if this site has no server. Throws in words when the server
	 * knows no such pilot, which is also what it says of one who shares
	 * nothing.
	 */
	async pilot(handle: string, after?: string): Promise<PilotPage | undefined> {
		if (!(await this.#present())) return undefined;
		const answer = await this.#get(pilotPagePath(handle, after));
		return pilotPage(answer.status, answer.body);
	}

	/**
	 * The pilots of a season, ranked by the sum of the points of their best
	 * public flights. `season` is the name of one of the seasons the core
	 * offers at `nowUnixS`; any other text, or none, means the newest.
	 * With `near`, only flights that took off near that place count.
	 * Nothing if this site has no server.
	 */
	async ranking(
		season?: string,
		near?: Coordinate,
		nowUnixS = Date.now() / 1000
	): Promise<SeasonRanking | undefined> {
		if (!(await this.#present())) return undefined;
		const { seasons } = rankingSeasons(nowUnixS);
		const chosen = seasons.find((offered) => offered.name === season) ?? seasons[0];
		const area = near ? flownFromHere(near.lat, near.lon) : undefined;
		const answer = await this.#get(rankingPath(area, chosen.fromUnixS, chosen.toUnixS));
		return { seasons, season: chosen, ranking: ranking(answer.status, answer.body) };
	}

	/**
	 * Fetches a flight by its address and prepares it for display: first
	 * what the list says about it, then its file. Nothing if this site has
	 * no server. Throws in words when the flight is no longer public, or
	 * its file is not a flight.
	 */
	async open(id: string): Promise<OpenedFlight | undefined> {
		if (!(await this.#present())) return undefined;
		const about = await this.#get(publicFlightPath(id));
		const row = publicFlight(about.status, about.body);

		const file = await this.#get(row.filePath);
		// Not the file: the core puts into words what the server said
		// instead, which it does by throwing.
		if (file.status < 200 || file.status > 299) publicFlight(file.status, file.body);
		const text = flightFileText(file.body);
		const flight = flightFromFile(text);
		let analysis: FlightAnalysis | undefined;
		try {
			analysis = flightAnalysisFromFile(text);
		} catch {
			// Without the analysis the details are shorter, not wrong.
		}
		return { row, flight, analysis };
	}
}
