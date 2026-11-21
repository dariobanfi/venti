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
import { isInWords } from '#lib/core/index.ts';
import { span } from '#lib/community/pages.ts';
import { asked, NO_CHOICE } from '#lib/sites/filter.ts';

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

export interface WeatherPanel {
	/** The thermal map's first day: "Today, ICON-D2 of 06:00" and its note. */
	thermals: { name: string; model: string; runUnixS: number; note: string } | undefined;
	/** The radar's newest moment in the core's words: "Rain radar, 14:15, 4 minutes ago." */
	radar: string | undefined;
}

export class HomeModel {
	flights = $state.raw<Panel<FlightsPanel>>(LOADING);
	ranking = $state.raw<Panel<RankingPanel>>(LOADING);
	live = $state.raw<Panel<LiveAnswer>>(LOADING);
	community = $state.raw<Panel<CommunityPanel>>(LOADING);
	weather = $state.raw<Panel<WeatherPanel>>(LOADING);

	#core: CoreClient;
	#asked = false;

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
			void this.#fill('weather', () => this.#weather());
		});
	}

	/** Asks again for one panel, after it failed. */
	again(panel: 'flights' | 'ranking' | 'live' | 'community' | 'weather', today: string): void {
		const ask = {
			flights: () => this.#flights(today),
			ranking: () => this.#ranking(),
			live: () => this.#core.live(HOME_BOX),
			community: () => this.#community(),
			weather: () => this.#weather()
		}[panel] as () => Promise<never>;
		void this.#fill(panel, ask);
	}

	async #fill<K extends 'flights' | 'ranking' | 'live' | 'community' | 'weather'>(
		panel: K,
		ask: () => Promise<unknown>
	): Promise<void> {
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

	async #ranking(): Promise<RankingPanel | undefined> {
		const ranked = await this.#core.ranking(undefined);
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

	async #weather(): Promise<WeatherPanel | undefined> {
		const [thermals, radar] = await Promise.all([
			this.#core.thermalMap().catch(() => undefined),
			this.#core.radar().catch(() => undefined)
		]);
		if (!thermals && !radar) return undefined;
		const day = thermals?.days[0];
		const moment = radar?.moments[radar.firstMoment];
		return {
			thermals: day
				? { name: day.name, model: day.model, runUnixS: day.runUnixS, note: day.note }
				: undefined,
			radar: moment?.label
		};
	}
}

function words(error: unknown): string {
	const said = error instanceof Error ? error.message : String(error);
	return isInWords(error) ? said : `Venti could not be reached. ${said}`;
}
