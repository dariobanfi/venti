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
	type Bounds,
	compareFlightFiles,
	type ComparedFlight,
	type Coordinate,
	flightAnalysisFromFile,
	flownFromHere,
	type FlightAnalysis,
	flightFromFile,
	type FlightView,
	type FlightSearch,
	flightSearchBounds,
	type FlightSearchBounds,
	flightSearchWords,
	pilotFlightsSearchPath,
	pilotPage,
	type PilotPage,
	pilotFlightsPath,
	pilotPagePath,
	pilotsFound,
	type PilotsFound,
	pilotsNear,
	type PilotsNear,
	pilotsNearPath,
	pilotsSearchPath,
	pilotStatistics,
	type PilotStatistics,
	pilotStatisticsPath,
	publicFlight,
	type PublicFlight,
	publicFlightPath,
	publicFlights,
	type PublicFlights,
	type PublicFlightsOrder,
	publicFlightsPath,
	publicFlightsSearchPath,
	ranking,
	type Ranking,
	type RankingMonthChoice,
	rankingMonthPath,
	rankingPath,
	type RankingSeason,
	rankingSearchPath,
	rankingSeasons,
	type RouteKind,
	routeKindWords
} from './pkg/venti_core_wasm.js';
import {
	isSearching,
	only,
	ROUTE_KINDS,
	SEARCH_PARTS,
	type SearchPart
} from '#lib/community/flight-search.ts';
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
		throw unreachable('Venti could not be reached for other pilots’ flights.');
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
	/** Only flights that took off inside this part of the map. */
	area?: Bounds;
	/** What else the pilot chose to search by: a radius, a drawn area, a class and more. */
	search?: FlightSearch;
}

/** One chip of a search: the choice it shows, in the core's words. */
export interface SearchChip {
	part: SearchPart;
	words: string;
}

/** What a search's form offers: the server's bounds, and the kinds of route by name. */
export interface SearchChoices {
	bounds: FlightSearchBounds;
	kinds: { kind: RouteKind; name: string }[];
}

/** The chips of a search, one a choice, in the core's words and order. */
export function searchChips(search: FlightSearch): SearchChip[] {
	return SEARCH_PARTS.flatMap((part) => {
		const words = flightSearchWords(only(search, part));
		return words.length > 0 ? [{ part, words: words[0] }] : [];
	});
}

/** What a search's form offers. */
export function searchChoices(): SearchChoices {
	return {
		bounds: flightSearchBounds(),
		kinds: ROUTE_KINDS.map((kind) => ({ kind, name: routeKindWords(kind) }))
	};
}

/**
 * Why a search cannot be asked, in the core's words; nothing where it can.
 * The core says so as it writes the path.
 */
export function searchRefused(search: FlightSearch): string | undefined {
	try {
		publicFlightsSearchPath('newest', undefined, undefined, undefined, search);
		return undefined;
	} catch (error) {
		return error instanceof Error ? error.message : String(error);
	}
}

/** An error whose message the page shows as it is, as the core's are. */
function inWords(message: string): Error {
	const error = new Error(message);
	error.name = 'Server';
	return error;
}

/**
 * A search the server refused or gave up on, in words; nothing for any
 * other answer, which the core reads. The server gives a read up after two
 * seconds (`503`, `unavailable`): a narrower search is quicker. A
 * search it refuses (`400`) it says why of, which the page shows.
 */
function searchFailure(status: number, body: Uint8Array): Error | undefined {
	if (status === 503) {
		return inWords(
			'The search took too long. Narrow it: a smaller area, fewer days or more choices.'
		);
	}
	if (status !== 400) return undefined;
	try {
		const said = JSON.parse(new TextDecoder().decode(body)) as { error?: { message?: unknown } };
		const message = said.error?.message;
		if (typeof message === 'string' && message.trim()) {
			const text = message.trim();
			return inWords(`${text[0].toUpperCase()}${text.slice(1)}${/[.!?]$/.test(text) ? '' : '.'}`);
		}
	} catch {
		// Not the server's words: the core says what it can.
	}
	return undefined;
}

/** The first and the first after a calendar month by UTC, as the ranking's periods write one ("2026-05"). */
function monthSpan(month: string): { fromUnixS: number; toUnixS: number } | undefined {
	const parts = /^(\d{4})-(\d{2})$/.exec(month);
	if (!parts) return undefined;
	const year = Number(parts[1]);
	const index = Number(parts[2]) - 1;
	return {
		fromUnixS: Date.UTC(year, index, 1) / 1000,
		toUnixS: Date.UTC(year, index + 1, 1) / 1000
	};
}

/** A flight to lay beside another: one of the pilot's own, or a public one, by its address. */
export type FlightRef = { own: string } | { public: string };

/** A flight laid beside another, with whose it is when it is another pilot's. */
export type Compared = ComparedFlight & { pilot?: string };

/** The pilots of a season ranked, with the seasons there are to choose from. */
export interface SeasonRanking {
	/** The seasons a page offers, the newest first, and all years together last. */
	seasons: RankingSeason[];
	/** The season this is the ranking of, or the one its month lies in. */
	season: RankingSeason;
	/** The months of that season that have a ranking, as the core names them. None for all years. */
	months: RankingMonthChoice[];
	/** The month this is the ranking of, when it is of one. */
	month: RankingMonthChoice | undefined;
	/** The whole year as the first choice beside its months: "All of 2026". Empty for all years. */
	wholeName: string;
	/** The period in the core's words: "2026: 10 flights", "May 2026: 4 flights". Empty where the server does not say. */
	words: string;
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
	/** Venti's clock, in seconds, or 0 while it is not known: the seasons are offered by it. */
	#now: () => Promise<number>;

	constructor(
		present: () => Promise<boolean> = async () => true,
		get: Get = getFromThisOrigin,
		now: () => Promise<number> = async () => Date.now() / 1000
	) {
		this.#present = present;
		this.#get = get;
		this.#now = now;
	}

	/**
	 * One page of everyone's public flights. Nothing if this site has no
	 * server. Throws, with words for the pilot, when the server gives no
	 * usable answer.
	 */
	async flights(asked: PublicFlightsAsked): Promise<PublicFlights | undefined> {
		if (!(await this.#present())) return undefined;
		const area = asked.near ? flownFromHere(asked.near.lat, asked.near.lon) : asked.area;
		const search = asked.search;
		const answer = await this.#get(
			search && isSearching(search)
				? publicFlightsSearchPath(
						asked.order,
						area,
						asked.fromUnixS,
						asked.toUnixS,
						search,
						asked.after
					)
				: publicFlightsPath(asked.order, area, asked.fromUnixS, asked.toUnixS, asked.after)
		);
		if (search && isSearching(search)) {
			const failure = searchFailure(answer.status, answer.body);
			if (failure) throw failure;
		}
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
	 * One page of a pilot's public flights of a season: those that began at
	 * or after the one moment and before the other, as a season of the
	 * pilot's totals names them. Nothing if this site has no server.
	 */
	async pilotFlights(
		handle: string,
		fromUnixS?: number,
		toUnixS?: number,
		after?: string,
		search?: FlightSearch
	): Promise<PublicFlights | undefined> {
		if (!(await this.#present())) return undefined;
		const searching = search !== undefined && isSearching(search);
		const answer = await this.#get(
			searching
				? pilotFlightsSearchPath(handle, 'newest', fromUnixS, toUnixS, search, after)
				: pilotFlightsPath(handle, fromUnixS, toUnixS, after)
		);
		if (searching) {
			const failure = searchFailure(answer.status, answer.body);
			if (failure) throw failure;
		}
		return publicFlights(answer.status, answer.body);
	}

	/**
	 * The pilots whose handle or name holds a text, as lines for a list.
	 * The core refuses a text of fewer than two letters in words, before
	 * anything is asked of the server. Nothing if this site has no server.
	 */
	async pilots(text: string): Promise<PilotsFound | undefined> {
		if (!(await this.#present())) return undefined;
		const answer = await this.#get(pilotsSearchPath(text));
		return pilotsFound(answer.status, answer.body);
	}

	/**
	 * The pilots whose public flights took off near a place in the last
	 * months, as the core's lines: the server takes the place to about a
	 * kilometre and keeps it nowhere. Nothing if this site has no server.
	 */
	async pilotsNear(place: Coordinate): Promise<PilotsNear | undefined> {
		if (!(await this.#present())) return undefined;
		const answer = await this.#get(pilotsNearPath(place.lat, place.lon));
		return pilotsNear(answer.status, answer.body);
	}

	/**
	 * What a pilot's public flights add up to: all of them and each year,
	 * as the core's rows. Nothing if this site has no server.
	 */
	async statistics(handle: string): Promise<PilotStatistics | undefined> {
		if (!(await this.#present())) return undefined;
		const answer = await this.#get(pilotStatisticsPath(handle));
		return pilotStatistics(answer.status, answer.body);
	}

	/**
	 * The pilots of a season, ranked by the sum of the points of their best
	 * public flights. `season` is the name of one of the seasons the core
	 * offers at `nowUnixS`; any other text, or none, means the newest.
	 * With `near`, only flights that took off near that place count. With
	 * `month`, as the ranking's periods write one ("2026-05"), the ranking
	 * is of that month, and the season the year it lies in.
	 * Nothing if this site has no server.
	 */
	async ranking(
		season?: string,
		near?: Coordinate,
		nowUnixS?: number,
		month?: string,
		search?: FlightSearch
	): Promise<SeasonRanking | undefined> {
		if (!(await this.#present())) return undefined;
		const { seasons } = rankingSeasons(nowUnixS ?? (await this.#now()));
		const asked = seasons.find((offered) => offered.name === season) ?? seasons[0];
		const area = near ? flownFromHere(near.lat, near.lon) : undefined;
		const searching = search !== undefined && isSearching(search);
		// The core has no path for a month and a search at once: the month
		// is then asked as its span, which is a calendar month by UTC.
		const span = month ? monthSpan(month) : undefined;
		let path: string;
		if (searching && search) {
			path = span
				? rankingSearchPath(area, span.fromUnixS, span.toUnixS, search)
				: rankingSearchPath(area, asked.fromUnixS, asked.toUnixS, search);
		} else {
			path = month
				? rankingMonthPath(area, month)
				: rankingPath(area, asked.fromUnixS, asked.toUnixS);
		}
		const answer = await this.#get(path);
		if (searching) {
			const failure = searchFailure(answer.status, answer.body);
			if (failure) throw failure;
		}
		const ranked = ranking(answer.status, answer.body);
		// The year a month lies in is the one whose months hold it.
		const year = month
			? ranked.periods.find((period) => period.months.some((one) => one.month === month))
			: ranked.periods.find((period) => period.fromUnixS === asked.fromUnixS);
		const of = year?.months.find((one) => one.month === month);
		const chosen =
			month && year
				? (seasons.find((offered) => offered.name === year.name) ?? {
						name: year.name,
						fromUnixS: year.fromUnixS,
						toUnixS: year.toUnixS
					})
				: asked;
		return {
			seasons,
			season: chosen,
			months: year?.months ?? [],
			month: of,
			wholeName: year?.wholeName ?? '',
			words: of?.words ?? year?.words ?? '',
			ranking: ranked
		};
	}

	/**
	 * Fetches a flight by its address and prepares it for display: first
	 * what the list says about it, then its file. Nothing if this site has
	 * no server. Throws in words when the flight is no longer public, or
	 * its file is not a flight.
	 */
	async open(id: string): Promise<OpenedFlight | undefined> {
		const fetched = await this.file(id);
		if (!fetched) return undefined;
		const { row, text } = fetched;
		const flight = flightFromFile(text);
		let analysis: FlightAnalysis | undefined;
		try {
			analysis = flightAnalysisFromFile(text);
		} catch {
			// Without the analysis the details are shorter, not wrong.
		}
		return { row, flight, analysis };
	}

	/**
	 * Lays a flight beside another: `first` is the one on screen. A flight of
	 * the pilot's own is read from the logbook by `ownText`, a public one
	 * fetched; nothing is kept. Comes back with the other flight, how the
	 * two lie to each other, and whose the other is when it is a pilot's
	 * public flight. Throws in words when a flight cannot be had.
	 */
	async compare(
		first: FlightRef,
		other: FlightRef,
		ownText: (id: string) => Promise<string>
	): Promise<Compared> {
		const read = async (ref: FlightRef): Promise<{ text: string; pilot?: string }> => {
			if ('own' in ref) return { text: await ownText(ref.own) };
			const fetched = await this.file(ref.public);
			if (!fetched) {
				throw unreachable(
					'Other pilots’ flights are kept by Venti, and this copy of the app is served without Venti behind it.'
				);
			}
			return { text: fetched.text, pilot: fetched.row.pilot };
		};
		const [one, two] = await Promise.all([read(first), read(other)]);
		return { ...compareFlightFiles(one.text, two.text), pilot: two.pilot };
	}

	/**
	 * A public flight's file as text, with what the list says about it.
	 * Nothing if this site has no server. Throws in words when the flight
	 * is no longer public.
	 */
	async file(id: string): Promise<{ row: PublicFlight; text: string } | undefined> {
		if (!(await this.#present())) return undefined;
		const about = await this.#get(publicFlightPath(id));
		const row = publicFlight(about.status, about.body);

		const file = await this.#get(row.filePath);
		// Not the file: the core puts into words what the server said
		// instead, which it does by throwing.
		if (file.status < 200 || file.status > 299) publicFlight(file.status, file.body);
		return { row, text: flightFileText(file.body) };
	}
}
