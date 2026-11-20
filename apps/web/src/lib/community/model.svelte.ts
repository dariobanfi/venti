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
	OpenedFlight,
	PublicFlight,
	PublicFlightsOrder,
	RankedPilot,
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

/** One pilot and the flights that pilot shares with everyone. */
export interface PilotState {
	handle: string;
	/** The name the pilot is shown by, once the server has said it. */
	name: string | undefined;
	list: ListState;
}

/** What a screen is shown of the ranking of a season. */
export interface RankingState {
	/** The season asked for, by its name. Empty asks for the newest. */
	asked: string;
	/** Whether the server has answered. */
	known: boolean;
	/** The seasons there are to choose from, by name. None before the first answer. */
	seasons: string[];
	/** The season the ranking is of. */
	season: string;
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
}

export const NOT_RANKED: RankingState = {
	asked: '',
	known: false,
	seasons: [],
	season: '',
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
	/** The first three of this year's ranking. */
	top: RankedPilot[];
	/** The season `top` is of: "2026". */
	season: string;
	/** Why it is missing, in words. */
	problem: string | undefined;
	/** Whether the app turned out to have no Venti behind it. */
	absent: boolean;
}

/** How many flights and pilots the front shows. */
const ON_THE_FRONT = 3;

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

	#core: CoreClient;
	#everyone: PagedFlights | undefined;
	#ofPilot: PagedFlights | undefined;
	/** The season whose ranking was last asked for. Nothing before the first. */
	#ranked: string | undefined;
	#ofPlace: PagedFlights | undefined;
	/** Where the place's flights are counted around, once the server has said which site is there. */
	#centre: Promise<Coordinate> | undefined;
	/** The question of the place's ranking last asked. */
	#placeRanked: object | undefined;

	constructor(core: CoreClient) {
		this.#core = core;
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
			const list = new PagedFlights(
				(after) => this.#core.publicFlights({ order: asked.order, fromUnixS, toUnixS, after }),
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

	/** Shows the public flights of the pilot of that handle, from their top. */
	showPilot(handle: string): void {
		untrack(() => {
			if (this.#ofPilot && this.pilot?.handle === handle) return;
			this.pilot = { handle, name: undefined, list: NOT_LISTED };
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
	 * name is the newest. A ranking that is there for the same season, or on
	 * its way, is left as it is.
	 */
	showRanking(season: string): void {
		untrack(() => {
			const there = this.ranking.known || this.ranking.loading;
			if (this.#ranked === season && there) return;
			void this.#rank(season);
		});
	}

	/** Asks again: for a ranking that could not be fetched. */
	rankAgain(): void {
		void this.#rank(this.ranking.asked);
	}

	async #rank(season: string): Promise<void> {
		this.#ranked = season;
		// The seasons stay to choose from while another one is fetched.
		const seasons = this.ranking.seasons;
		this.ranking = { ...NOT_RANKED, asked: season, seasons, season, loading: true };
		const ranking = await rankingOf(() => this.#core.ranking(season || undefined), season, seasons);
		// Another season has been asked for since.
		if (this.#ranked === season) this.ranking = ranking;
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
				top: ranked?.ranking.pilots.slice(0, ON_THE_FRONT) ?? [],
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
	seasons: string[]
): Promise<RankingState> {
	try {
		const answer = await ask();
		return answer
			? {
					asked: season,
					known: true,
					seasons: answer.seasons.map((offered) => offered.name),
					season: answer.season.name,
					pilots: answer.ranking.pilots,
					rule: answer.ranking.rule,
					loading: false,
					problem: undefined,
					absent: false
				}
			: { ...NOT_RANKED, asked: season, absent: true };
	} catch (error) {
		return { ...NOT_RANKED, asked: season, seasons, season, problem: words(error) };
	}
}
