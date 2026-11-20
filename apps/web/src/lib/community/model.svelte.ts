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
import type { CoreClient, OpenedFlight, RankedPilot } from '#lib/core/index.ts';
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

export class CommunityModel {
	/** The ranking of the server's pilots in the season last asked for. */
	ranking = $state.raw<RankingState>(NOT_RANKED);

	/** What is asked of everyone's flights: their order, and the days. */
	asked = $state.raw<Asked>(EVERYTHING);

	/** Everyone's flights, as far as they are listed. */
	list = $state.raw<ListState>(NOT_LISTED);

	/** The pilot whose flights are looked at, if one is. */
	pilot = $state.raw<PilotState>();

	#core: CoreClient;
	#everyone: PagedFlights | undefined;
	#ofPilot: PagedFlights | undefined;
	/** The season whose ranking was last asked for. Nothing before the first. */
	#ranked: string | undefined;

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
		try {
			const answer = await this.#core.ranking(season || undefined);
			// Another season has been asked for since.
			if (this.#ranked !== season) return;
			this.ranking = answer
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
			if (this.#ranked !== season) return;
			this.ranking = { ...NOT_RANKED, asked: season, seasons, season, problem: words(error) };
		}
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
