/**
 * The home page: what is going on, on Venti and in the air, in the first
 * region. Each panel is asked for on its own and stands on its own: one
 * that fails says so and the others are shown.
 *
 * Every word about a rule, a verdict or a count is the core's; this only
 * asks and holds the answers. Today's take-offs are `App.today`'s, the
 * same the Today screen shows; the alarms are `App.alerts`'.
 */

import { untrack } from 'svelte';
import type {
	Bounds,
	CoreClient,
	LiveAnswer,
	PublicFlight,
	RankedPilot,
	SiteCandidateView,
	SiteView
} from '#lib/core/index.ts';
import { inWordsOr } from '#lib/core/index.ts';
import { span } from '#lib/community/pages.ts';
import { asked, NO_CHOICE } from '#lib/sites/filter.ts';
import type { SeasonRanking } from '#lib/core/community-source.ts';
import { type Achievement, achievements, RECENT_DAYS } from './achievements.ts';

/**
 * The part of the map the home page looks at: the first region, from the
 * Zugspitze to the Chiemgau. Smaller than the server's limits for others in
 * the air (10°) and for take-offs without a site (2°).
 */
export const HOME_BOX: Bounds = { south: 47.2, west: 10.7, north: 47.85, east: 12.3 };

/** How many rows a panel lists. */
export const FLIGHTS_SHOWN = 10;
export const RANKED_SHOWN = 10;
export const COMMUNITY_SHOWN = 4;

/** A panel's answer: on its way, there, not to be had (and why), or no Venti behind the app. */
export type Panel<T> =
	| { phase: 'loading' }
	| { phase: 'loaded'; value: T }
	| { phase: 'failed'; problem: string }
	| { phase: 'absent' };

const LOADING = { phase: 'loading' } as const;

/** The panels the model asks for. */
type Kind = 'flights' | 'ranking' | 'live' | 'community' | 'achievements';

export interface FlightsPanel {
	flights: PublicFlight[];
	/** Whether these are today's best; else the newest of any day. */
	ofToday: boolean;
}

export interface RankingPanel {
	season: string;
	pilots: RankedPilot[];
}

export interface CommunityPanel {
	/** Places from pilots, the one added, changed or confirmed last first. */
	places: SiteView[];
	/** How many places from pilots the region has. */
	placesCount: number;
	/** Where pilots take off and no site stands, the one with most pilots first. */
	candidates: SiteCandidateView[];
	candidatesCount: number;
	/** The core's words when there are more than one answer lists, or empty. */
	candidatesMore: string;
}

export class HomeModel {
	flights = $state.raw<Panel<FlightsPanel>>(LOADING);
	ranking = $state.raw<Panel<RankingPanel>>(LOADING);
	live = $state.raw<Panel<LiveAnswer>>(LOADING);
	community = $state.raw<Panel<CommunityPanel>>(LOADING);
	/** The season's flights that stand out, as large cards. */
	achievements = $state.raw<Panel<Achievement[]>>(LOADING);

	#core: CoreClient;
	#asked = false;
	/** The newest season's ranking, asked for once for its panel and the achievements. */
	#season: Promise<SeasonRanking | undefined> | undefined;

	constructor(core: CoreClient) {
		this.#core = core;
	}

	/** Asks for every panel, once; `today` is the pilot's date, "2026-11-21". */
	show(today: string): void {
		untrack(() => {
			if (this.#asked) return;
			this.#asked = true;
			void this.#fill('flights', () => this.#flights(today));
			void this.#fill('ranking', () => this.#ranking());
			void this.#fill('live', () => this.#core.live(HOME_BOX));
			void this.#fill('community', () => this.#community());
			void this.#fill('achievements', () => this.#achievements());
		});
	}

	/** Asks again for one panel, after it failed. */
	again(panel: Kind, today: string): void {
		if (panel === 'ranking' || panel === 'achievements') this.#season = undefined;
		const ask = {
			flights: () => this.#flights(today),
			ranking: () => this.#ranking(),
			live: () => this.#core.live(HOME_BOX),
			community: () => this.#community(),
			achievements: () => this.#achievements()
		}[panel] as () => Promise<never>;
		void this.#fill(panel, ask);
	}

	async #fill<K extends Kind>(panel: K, ask: () => Promise<unknown>): Promise<void> {
		this[panel] = LOADING;
		try {
			const value = await ask();
			this[panel] = (
				value === undefined ? { phase: 'absent' } : { phase: 'loaded', value }
			) as this[K];
		} catch (error) {
			this[panel] = { phase: 'failed', problem: words(error) } as this[K];
		}
	}

	async #flights(today: string): Promise<FlightsPanel | undefined> {
		const { fromUnixS, toUnixS } = span({ from: today, to: today });
		const best = await this.#core.publicFlights({ order: 'best', fromUnixS, toUnixS });
		if (!best) return undefined;
		if (best.flights.length > 0) {
			return { flights: best.flights.slice(0, FLIGHTS_SHOWN), ofToday: true };
		}
		const newest = await this.#core.publicFlights({ order: 'newest' });
		return { flights: (newest?.flights ?? []).slice(0, FLIGHTS_SHOWN), ofToday: false };
	}

	/** The newest season's ranking, asked for once until a panel asks again. */
	#seasonRanking(): Promise<SeasonRanking | undefined> {
		this.#season ??= this.#core.ranking(undefined);
		return this.#season;
	}

	async #ranking(): Promise<RankingPanel | undefined> {
		const ranked = await this.#seasonRanking();
		if (!ranked) return undefined;
		return { season: ranked.season.name, pilots: ranked.ranking.pilots.slice(0, RANKED_SHOWN) };
	}

	async #community(): Promise<CommunityPanel | undefined> {
		const [sites, candidates] = await Promise.all([
			this.#core.sites(
				HOME_BOX,
				asked({ ...NO_CHOICE, fromPilots: true }, { measuredDeg: undefined })
			),
			this.#core.siteCandidates(HOME_BOX)
		]);
		if (!sites) return undefined;
		const fromPilots = sites.sites.filter((site) => site.byPilots?.fromPilots);
		const lately = (site: SiteView) =>
			Math.max(
				site.byPilots?.changedUnixS ?? 0,
				site.byPilots?.confirmedLatestDay
					? Date.parse(`${site.byPilots.confirmedLatestDay}T12:00:00Z`) / 1000
					: 0
			);
		return {
			places: fromPilots
				.toSorted((one, other) => lately(other) - lately(one))
				.slice(0, COMMUNITY_SHOWN),
			placesCount: fromPilots.length,
			candidates: candidates.candidates.slice(0, COMMUNITY_SHOWN),
			candidatesCount: candidates.candidates.length,
			candidatesMore: candidates.moreWords
		};
	}

	/**
	 * The season's longest flight, its best triangle and the best of the
	 * last days (or the newest), from everyone's flights by points.
	 */
	async #achievements(): Promise<Achievement[] | undefined> {
		const ranked = await this.#seasonRanking();
		if (!ranked) return undefined;
		const { season } = ranked;
		const recentFrom = Math.floor(Date.now() / 1000) - RECENT_DAYS * 86_400;
		const [best, recent, newest] = await Promise.all([
			this.#core.publicFlights({
				order: 'best',
				fromUnixS: season.fromUnixS,
				toUnixS: season.toUnixS
			}),
			this.#core.publicFlights({ order: 'best', fromUnixS: recentFrom }),
			this.#core.publicFlights({ order: 'newest' })
		]);
		if (!best) return undefined;
		return achievements(
			season.fromUnixS === undefined ? '' : season.name,
			best.flights,
			recent?.flights ?? [],
			newest?.flights ?? []
		);
	}
}

function words(error: unknown): string {
	return inWordsOr('Venti could not be reached.', error);
}
