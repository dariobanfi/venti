/**
 * Other pilots' flights as the screens see them: everyone's public flights
 * in the order and for the days the pilot chose, one pilot's flights, the
 * ranking of the pilots in a season, and one flight opened from its file.
 *
 * `pages.ts` holds the rules. This class gives them the page: the core in
 * its worker to ask, and what the screens watch. It lives as long as the
 * app is open, so the list is still there when the pilot comes back to it
 * from a flight. Nothing of another pilot's is kept beyond that.
 */

import { untrack } from 'svelte';
import type {
	Bounds,
	Compared,
	Coordinate,
	FlightRef,
	CoreClient,
	FlightSearch,
	OpenedFlight,
	PilotStatistics,
	PilotsNear,
	PilotsWords,
	PublicFlight,
	PublicFlightsOrder,
	RankedPilot,
	RankingMonthChoice,
	SearchChip,
	SearchChoices,
	SeasonRanking,
	SiteAtPlace
} from '#lib/core/index.ts';
import {
	type Asked,
	EVERYTHING,
	type ListState,
	NOT_LISTED,
	PagedFlights,
	sameAsked,
	span,
	words
} from './pages';
import { NOT_SEARCHED, PilotSearch, type SearchState } from './search';
import { isSearching, NO_SEARCH, sameSearch, searchParams } from './flight-search';

/** One pilot and the flights that pilot shares with everyone. */
export interface PilotState {
	handle: string;
	/** The name the pilot is shown by, once the server has said it. */
	name: string | undefined;
	list: ListState;
	/** What the pilot's public flights add up to, once the server has said. */
	statistics: PilotStatistics | undefined;
	/** Why the totals are missing, in words. The flights are listed all the same. */
	statisticsProblem: string | undefined;
	/** The season the list is narrowed to, by its name; empty for all years. */
	season: string;
	/** The bounds of that season, for a search within it. */
	span: { fromUnixS: number | undefined; toUnixS: number | undefined };
	/** What else the list is searched by. */
	search: FlightSearch;
}

/** A season of a pilot's totals, as the list is narrowed to it. */
export interface PilotSeason {
	name: string;
	fromUnixS: number | undefined;
	toUnixS: number | undefined;
}

/** What a screen is shown of the ranking of a season. */
export interface RankingState {
	/** The season asked for, by its name. Empty asks for the newest. */
	asked: string;
	/** Whether the server has answered. */
	known: boolean;
	/** The seasons there are to choose from, by name. None before the first answer. */
	seasons: string[];
	/** The season the ranking is of, or the one its month lies in. */
	season: string;
	/** The month asked for, as the ranking's periods write one ("2026-05"). Empty for the whole season. */
	askedMonth: string;
	/** The months of the season that have a ranking, as the core names them. */
	months: RankingMonthChoice[];
	/** The month the ranking is of, when it is of one. */
	month: RankingMonthChoice | undefined;
	/** The whole year as the first choice beside its months, in the core's words: "All of 2026". */
	wholeName: string;
	/** What the ranking is of, as the core calls it: "2026", "May 2026". */
	title: string;
	/** The period in the core's words: "May 2026: 1 flight". Empty where the server does not say. */
	words: string;
	/** The pilots, the one with the most points first. */
	pilots: RankedPilot[];
	/** The rule in a sentence, for under the list. */
	rule: string;
	/** Whether the ranking is being fetched. */
	loading: boolean;
	/** Why it is missing, in words for the pilot. */
	problem: string | undefined;
	/** Whether this site turned out to have no server to ask. */
	absent: boolean;
	/** What else the flights ranked are searched by. */
	search?: FlightSearch;
	/** The country whose pilots alone are ranked, two letters; empty or none for all. */
	pilotCountry?: string;
}

export const NOT_RANKED: RankingState = {
	asked: '',
	known: false,
	seasons: [],
	season: '',
	askedMonth: '',
	months: [],
	month: undefined,
	wholeName: '',
	title: '',
	words: '',
	pilots: [],
	rule: '',
	loading: false,
	problem: undefined,
	absent: false
};

/** What a place's page asks for: the place, perhaps a site there, and how to list and rank. */
export interface PlaceAsked {
	at: Coordinate;
	/** The identifier of the site the page is about. Empty for the take-off nearest to the place. */
	site: string;
	order: PublicFlightsOrder;
	/** The season of the ranking, by name. Empty for the newest. */
	season: string;
}

/**
 * A place's page: the flying site there, as one answer of the server's,
 * and, asked for apart, the flights that took off around it and the
 * ranking of their pilots.
 */
export interface PlaceState {
	asked: PlaceAsked;
	/** Whether the server has said which site is there, if any. */
	siteKnown: boolean;
	/** The site, with its source's credit. Nothing where the server knows none. */
	site: SiteAtPlace | undefined;
	/** Why the site is not known, in words for the pilot. The flights are asked for all the same. */
	siteProblem: string | undefined;
	/** The flights that took off around the place. */
	list: ListState;
	/** Their pilots, ranked. */
	ranking: RankingState;
	/** Whether this site turned out to have no server to ask. */
	absent: boolean;
}

function samePlace(one: PlaceAsked, other: PlaceAsked): boolean {
	return one.at.lat === other.at.lat && one.at.lon === other.at.lon && one.site === other.site;
}

/**
 * What a visitor is shown first of what is flown on Venti: the best flights
 * of the day, or the newest where nobody shared one today, and the top of
 * this year's ranking.
 */
export interface FrontState {
	/** The pilot's today, as a date field writes it: "2026-11-20". */
	today: string;
	/** Whether the server has answered for the flights. */
	known: boolean;
	/** The best of today's flights, or the newest when there are none today. At most three. */
	flights: PublicFlight[];
	/** Whether `flights` are today's best (or, when not, the newest of any day). */
	ofToday: boolean;
	/** The first ten of this year's ranking. */
	top: RankedPilot[];
	/** The season `top` is of: "2026". */
	season: string;
	/** Why it is missing, in words. */
	problem: string | undefined;
	/** Whether the app turned out to have no Venti behind it. */
	absent: boolean;
}

/** How many flights the front shows. */
const ON_THE_FRONT = 3;
/** How many pilots of the ranking it holds: a wide screen's side column has room for ten, a phone shows three. */
const TOP_OF_SEASON = 10;

export class CommunityModel {
	/** The ranking of the server's pilots in the season last asked for. */
	ranking = $state.raw<RankingState>(NOT_RANKED);

	/** What is asked of everyone's flights: their order, and the days. */
	asked = $state.raw<Asked>(EVERYTHING);

	/** Everyone's flights, as far as they are listed. */
	list = $state.raw<ListState>(NOT_LISTED);

	/** The pilot whose flights are looked at, if one is. */
	pilot = $state.raw<PilotState>();

	/** The place whose flights are looked at, if one is. */
	place = $state.raw<PlaceState>();

	/** What a visitor is shown first, once asked for. */
	front = $state.raw<FrontState>();

	/** The pilots found for what is typed into the search field. */
	search = $state.raw<SearchState>(NOT_SEARCHED);

	/** The core's words around pilots, their totals and the ranking, once asked for. */
	pilotsWords = $state.raw<PilotsWords>();

	#core: CoreClient;
	#everyone: PagedFlights | undefined;
	#ofPilot: PagedFlights | undefined;
	#search: PilotSearch;
	/** The season and month whose ranking was last asked for. Nothing before the first. */
	#ranked: string | undefined;
	#ofPlace: PagedFlights | undefined;
	/** Where the place's flights are counted around, once the server has said which site is there. */
	#centre: Promise<Coordinate> | undefined;
	/** The question of the place's ranking last asked. */
	#placeRanked: object | undefined;
	/** What a search's form offers, once asked for. */
	#choices: Promise<SearchChoices> | undefined;

	constructor(core: CoreClient) {
		this.#core = core;
		void core.pilotsWords().then(
			(said) => (this.pilotsWords = said),
			() => {}
		);
		this.#search = new PilotSearch(
			(text) => core.pilotsFound(text),
			(state) => (this.search = state)
		);
	}

	/** The chips of a search, one a choice, in the core's words. */
	searchChips(search: FlightSearch): Promise<SearchChip[]> {
		return this.#core.searchChips(search);
	}

	/** What a search's form offers: the server's bounds and the kinds of route, asked once. */
	searchChoices(): Promise<SearchChoices> {
		this.#choices ??= this.#core.searchChoices();
		return this.#choices;
	}

	/** Why a search cannot be asked, in the core's words; nothing where it can. */
	searchRefused(search: FlightSearch): Promise<string | undefined> {
		return this.#core.searchRefused(search);
	}

	/**
	 * Looks for the pilots whose name or handle holds what is typed, once
	 * the typing has rested. An empty text clears what was found.
	 */
	findPilots(typed: string): void {
		this.#search.look(typed);
	}

	/**
	 * The pilots whose public flights took off near a place, as the core's
	 * lines. Nothing if this site has no server; throws the core's words
	 * when the server refuses.
	 */
	pilotsNear(place: Coordinate): Promise<PilotsNear | undefined> {
		return this.#core.pilotsNear(place);
	}

	/**
	 * Shows everyone's flights in an order and for a span of days. A list
	 * that is there already for the same question is left as it is, with
	 * every page the pilot had fetched.
	 */
	show(asked: Asked): void {
		untrack(() => {
			if (this.#everyone && sameAsked(asked, this.asked)) return;
			this.asked = asked;
			this.list = NOT_LISTED;
			const { fromUnixS, toUnixS } = span(asked);
			const search = isSearching(asked.search) ? asked.search : undefined;
			const list = new PagedFlights(
				(after) =>
					this.#core.publicFlights({ order: asked.order, fromUnixS, toUnixS, after, search }),
				(state) => {
					// Another question has been asked since.
					if (list === this.#everyone) this.list = state;
				}
			);
			this.#everyone = list;
			void list.load();
		});
	}

	/** The next page of everyone's flights. */
	more(): void {
		void this.#everyone?.more();
	}

	/** Asks again from the top: for a list that could not be fetched. */
	again(): void {
		void this.#everyone?.load();
	}

	/**
	 * Shows the public flights of the pilot of that handle, from their top,
	 * as far as a search finds them. The same pilot with another search is
	 * the same page with another list.
	 */
	showPilot(handle: string, search: FlightSearch = NO_SEARCH): void {
		untrack(() => {
			const before = this.pilot;
			if (this.#ofPilot && before?.handle === handle) {
				if (!sameSearch(before.search, search)) this.#listPilot({ ...before, search });
				return;
			}
			this.pilot = {
				handle,
				name: undefined,
				list: NOT_LISTED,
				statistics: undefined,
				statisticsProblem: undefined,
				season: '',
				span: { fromUnixS: undefined, toUnixS: undefined },
				search
			};
			void this.#addUp(handle);
			if (isSearching(search)) {
				// The pilot's name comes with the first page of their flights.
				void this.#core.pilotPage(handle).then(
					(answer) => {
						if (answer && this.pilot?.handle === handle) {
							this.pilot = { ...this.pilot, name: answer.name };
						}
					},
					() => {}
				);
				this.#listPilot(this.pilot);
				return;
			}
			const list = new PagedFlights(
				async (after) => {
					const answer = await this.#core.pilotPage(handle, after);
					if (answer && list === this.#ofPilot && this.pilot) {
						this.pilot = { ...this.pilot, name: answer.name };
					}
					return answer;
				},
				(state) => {
					if (list === this.#ofPilot && this.pilot) this.pilot = { ...this.pilot, list: state };
				}
			);
			this.#ofPilot = list;
			void list.load();
		});
	}

	/**
	 * Narrows the pilot's list to a season of their totals, by its bounds;
	 * a season without bounds, all years, is the pilot's page again.
	 */
	narrowPilot(season: PilotSeason): void {
		const pilot = this.pilot;
		if (!pilot) return;
		const whole = season.fromUnixS === undefined && season.toUnixS === undefined;
		if (pilot.season === (whole ? '' : season.name)) return;
		this.#listPilot({
			...pilot,
			season: whole ? '' : season.name,
			span: { fromUnixS: season.fromUnixS, toUnixS: season.toUnixS }
		});
	}

	/** Lists the pilot's flights anew, of a season and as far as a search finds them. */
	#listPilot(pilot: PilotState): void {
		const { handle, span: within, search } = pilot;
		const whole = within.fromUnixS === undefined && within.toUnixS === undefined;
		this.pilot = { ...pilot, list: NOT_LISTED };
		const searching = isSearching(search);
		const list = new PagedFlights(
			(after) =>
				whole && !searching
					? this.#core.pilotPage(handle, after)
					: this.#core.pilotFlights(
							handle,
							within.fromUnixS,
							within.toUnixS,
							after,
							searching ? search : undefined
						),
			(state) => {
				if (list === this.#ofPilot && this.pilot) this.pilot = { ...this.pilot, list: state };
			}
		);
		this.#ofPilot = list;
		void list.load();
	}

	/** Asks again for what the pilot's flights add up to. */
	addUpAgain(): void {
		if (this.pilot) void this.#addUp(this.pilot.handle);
	}

	async #addUp(handle: string): Promise<void> {
		const mine = () => this.pilot?.handle === handle;
		if (mine() && this.pilot) this.pilot = { ...this.pilot, statisticsProblem: undefined };
		try {
			const statistics = await this.#core.pilotStatistics(handle);
			if (mine() && this.pilot) this.pilot = { ...this.pilot, statistics };
		} catch (error) {
			if (mine() && this.pilot) this.pilot = { ...this.pilot, statisticsProblem: words(error) };
		}
	}

	/** The next page of the pilot's flights. */
	morePilot(): void {
		void this.#ofPilot?.more();
	}

	/** Asks again for the pilot's flights from the top. */
	againPilot(): void {
		void this.#ofPilot?.load();
	}

	/**
	 * Shows the ranking of a season, named as the core names it; an empty
	 * name is the newest. With a month, as the ranking's periods write one
	 * ("2026-05"), the ranking of that month. With `pilotCountry`, two
	 * letters, of that country's pilots alone; empty for all. A ranking
	 * that is there for the same question, or on its way, is left as it is.
	 */
	showRanking(
		season: string,
		month = '',
		search: FlightSearch = NO_SEARCH,
		pilotCountry = ''
	): void {
		untrack(() => {
			const there = this.ranking.known || this.ranking.loading;
			const question = `${season}|${month}|${searchParams(search)}|${pilotCountry}`;
			if (this.#ranked === question && there) return;
			void this.#rank(season, month, search, pilotCountry);
		});
	}

	/** Asks again: for a ranking that could not be fetched. */
	rankAgain(): void {
		void this.#rank(
			this.ranking.asked,
			this.ranking.askedMonth,
			this.ranking.search,
			this.ranking.pilotCountry
		);
	}

	async #rank(
		season: string,
		month: string,
		search: FlightSearch = NO_SEARCH,
		pilotCountry = ''
	): Promise<void> {
		const question = `${season}|${month}|${searchParams(search)}|${pilotCountry}`;
		this.#ranked = question;
		// The seasons and the months stay to choose from while another one is fetched.
		const { seasons, months } = this.ranking;
		const before = month ? this.ranking.season : season;
		this.ranking = {
			...NOT_RANKED,
			asked: season,
			askedMonth: month,
			seasons,
			season: before,
			months: month ? months : [],
			loading: true,
			search,
			pilotCountry
		};
		const searching = isSearching(search);
		const ranking = await rankingOf(
			() =>
				this.#core.ranking(
					season || undefined,
					undefined,
					month || undefined,
					searching ? search : undefined,
					pilotCountry || undefined
				),
			season,
			seasons,
			month
		);
		// Something else has been asked for since.
		if (this.#ranked === question) this.ranking = { ...ranking, search, pilotCountry };
	}

	/**
	 * Asks for what a visitor is shown first: the best flights of `today`,
	 * the pilot's own date, and the top of this year's ranking. Asked once a
	 * day for as long as the app is open; again only after a failure.
	 */
	showFront(today: string): void {
		untrack(() => {
			const before = this.front;
			if (before && before.today === today && !before.problem) return;
			this.front = {
				today,
				known: false,
				flights: [],
				ofToday: true,
				top: [],
				season: '',
				problem: undefined,
				absent: false
			};
			void this.#fillFront(today);
		});
	}

	async #fillFront(today: string): Promise<void> {
		const { fromUnixS, toUnixS } = span({ from: today, to: today });
		try {
			const [best, ranked] = await Promise.all([
				this.#core.publicFlights({ order: 'best', fromUnixS, toUnixS }),
				this.#core.ranking(undefined)
			]);
			let flights = best?.flights ?? [];
			const ofToday = flights.length > 0;
			if (!ofToday && best) {
				flights = (await this.#core.publicFlights({ order: 'newest' }))?.flights ?? [];
			}
			if (this.front?.today !== today) return;
			this.front = {
				today,
				known: true,
				flights: flights.slice(0, ON_THE_FRONT),
				ofToday,
				top: ranked?.ranking.pilots.slice(0, TOP_OF_SEASON) ?? [],
				season: ranked?.season.name ?? '',
				problem: undefined,
				absent: !best
			};
		} catch (error) {
			if (this.front?.today === today) {
				this.front = { ...this.front, known: true, problem: words(error) };
			}
		}
	}

	/**
	 * Shows a place: the site there, the flights that took off around it in
	 * an order, and the ranking of their pilots in a season. The site is
	 * asked for first, because the flights are counted around the site
	 * where there is one. What is there already for the same question is
	 * left as it is.
	 */
	showPlace(asked: PlaceAsked): void {
		untrack(() => {
			const before = this.place;
			if (before && this.#centre && samePlace(before.asked, asked)) {
				const listed = before.asked.order === asked.order;
				const ranked = before.asked.season === asked.season;
				this.place = { ...before, asked };
				if (!listed) this.#listPlace(asked);
				if (!ranked) void this.#rankPlace(asked);
				return;
			}
			this.place = {
				asked,
				siteKnown: false,
				site: undefined,
				siteProblem: undefined,
				list: NOT_LISTED,
				ranking: { ...NOT_RANKED, asked: asked.season, loading: true },
				absent: false
			};
			const centre = this.#core.siteAt(asked.at, asked.site || undefined).then(
				(found) => {
					if (this.#centre === centre && this.place) {
						this.place = found
							? { ...this.place, siteKnown: true, site: found }
							: { ...this.place, siteKnown: true, absent: true };
					}
					return found?.site?.position ?? asked.at;
				},
				(error) => {
					if (this.#centre === centre && this.place) {
						this.place = { ...this.place, siteKnown: true, siteProblem: words(error) };
					}
					return asked.at;
				}
			);
			this.#centre = centre;
			this.#listPlace(asked);
			void this.#rankPlace(asked);
		});
	}

	/** The next page of the place's flights. */
	morePlace(): void {
		void this.#ofPlace?.more();
	}

	/** Asks again for the place's flights from the top. */
	againPlace(): void {
		void this.#ofPlace?.load();
	}

	/** Asks again for the place's ranking. */
	rankPlaceAgain(): void {
		if (this.place) void this.#rankPlace(this.place.asked);
	}

	#listPlace(asked: PlaceAsked): void {
		const centre = this.#centre;
		if (!centre || !this.place) return;
		this.place = { ...this.place, list: { ...NOT_LISTED, loading: true } };
		const list = new PagedFlights(
			async (after) => this.#core.publicFlights({ order: asked.order, near: await centre, after }),
			(state) => {
				if (list === this.#ofPlace && this.place) this.place = { ...this.place, list: state };
			}
		);
		this.#ofPlace = list;
		void list.load();
	}

	async #rankPlace(asked: PlaceAsked): Promise<void> {
		const centre = this.#centre;
		if (!centre || !this.place) return;
		const question = {};
		this.#placeRanked = question;
		const seasons = this.place.ranking.seasons;
		this.place = {
			...this.place,
			ranking: { ...NOT_RANKED, asked: asked.season, seasons, season: asked.season, loading: true }
		};
		const ranking = await rankingOf(
			() => centre.then((near) => this.#core.ranking(asked.season || undefined, near)),
			asked.season,
			seasons
		);
		if (this.#placeRanked === question && this.place) this.place = { ...this.place, ranking };
	}

	/**
	 * The public flights begun within twelve hours of a moment around a part
	 * of the map, the newest first: what others flew there that day, to lay
	 * beside a flight. Nothing from a site without Venti behind it.
	 */
	async flownAround(area: Bounds, startUnixS: number): Promise<PublicFlight[] | undefined> {
		const HALF_A_DAY_S = 12 * 3600;
		// A tenth of a degree more on every side: a flight from the next
		// mountain over is flown around the same place.
		const grown = {
			south: area.south - 0.1,
			west: area.west - 0.1,
			north: area.north + 0.1,
			east: area.east + 0.1
		};
		const page = await this.#core.publicFlights({
			order: 'newest',
			area: grown,
			fromUnixS: startUnixS - HALF_A_DAY_S,
			toUnixS: startUnixS + HALF_A_DAY_S
		});
		return page?.flights;
	}

	/** Lays one flight beside another, as `CoreClient.compareFlights` does. */
	compare(first: FlightRef, other: FlightRef): Promise<Compared> {
		return this.#core.compareFlights(first, other);
	}

	/**
	 * Fetches another pilot's flight by its address on the server and
	 * prepares it for display, like one of the logbook. Nothing comes back
	 * from a site without a server. Nothing is kept.
	 */
	open(id: string): Promise<OpenedFlight | undefined> {
		return this.#core.openPublicFlight(id);
	}
}

/**
 * A ranking as a screen is shown it, once the server has answered: the
 * pilots, the rule, and the seasons there are; or why there is none.
 */
async function rankingOf(
	ask: () => Promise<SeasonRanking | undefined>,
	season: string,
	seasons: string[],
	month = ''
): Promise<RankingState> {
	try {
		const answer = await ask();
		return answer
			? {
					asked: season,
					askedMonth: month,
					known: true,
					seasons: answer.seasons.map((offered) => offered.name),
					season: answer.season.name,
					months: answer.months,
					month: answer.month,
					wholeName: answer.wholeName,
					// The ranking's heading is the period's own title.
					// A month asked with a search is asked as its span, whose
					// title the server writes as days: the month's own is shorter.
					title:
						(month && answer.month?.title) ||
						answer.ranking.period.title ||
						(answer.month?.title ?? answer.season.name),
					words: answer.words,
					pilots: answer.ranking.pilots,
					rule: answer.ranking.rule,
					loading: false,
					problem: undefined,
					absent: false
				}
			: { ...NOT_RANKED, asked: season, askedMonth: month, absent: true };
	} catch (error) {
		return {
			...NOT_RANKED,
			asked: season,
			askedMonth: month,
			seasons,
			season,
			problem: words(error)
		};
	}
}
