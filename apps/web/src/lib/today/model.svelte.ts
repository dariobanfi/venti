/**
 * Today as the screen sees it: the pilot's own take-offs and those around a
 * place, and what the forecast says about flying from each, as the
 * iPhone's `TodayModel`.
 *
 * Which take-offs, and their order on a day, are the core's. The take-offs
 * are listed at once, and each forecast fills in as it arrives; with each
 * the core is asked for the order anew. The place is the middle of the
 * first region until the pilot asks for their own position; nothing asks
 * the browser before that.
 */

import { untrack } from 'svelte';
import type {
	Coordinate,
	CoreClient,
	Credit,
	SiteOutlook,
	TodayOrder,
	TodayTakeoff
} from '#lib/core/index.ts';
import { isInWords } from '#lib/core/index.ts';
import { standings } from './order.ts';

/** The middle of the first region, around the Tegernsee and the Brauneck. */
export const FIRST_REGION: Coordinate = { lat: 47.6, lon: 11.75 };

/**
 * A take-off as the core lists it (the site, whether it is the pilot's
 * own, how far away), and what the forecast says of it.
 */
export interface TodayPlace extends TodayTakeoff {
	/** Absent until the server has answered. */
	outlook: SiteOutlook | undefined;
	/** Why there is no forecast for it, in words. */
	problem: string | undefined;
}

export interface TodayState {
	/** Where the take-offs were looked for, and whether that is the pilot's own position. */
	place: Coordinate;
	nearYou: boolean;
	phase: 'idle' | 'loading' | 'loaded' | 'absent' | 'failed';
	problem: string | undefined;
	places: TodayPlace[];
	/**
	 * The core's order of the take-offs, one for each day the forecasts
	 * reach. Empty until the core has answered.
	 */
	orders: TodayOrder[];
	/** The core's names for the two groups: "Your take-offs", "Within 30 km". */
	keptTitle: string;
	nearTitle: string;
	/** The core's line when the pilot has more take-offs of their own than are listed, or empty. */
	keptMore: string;
	/** Whose data the take-offs are. */
	credits: Credit[];
	/** Why only the pilot's own are listed: what Venti said when asked for those around. */
	nearProblem: string | undefined;
	/** Now by Venti's clock, seconds since 1970-01-01 UTC, as of the last answer. */
	nowUnixS: number;
}

export const NOTHING_YET: TodayState = {
	place: FIRST_REGION,
	nearYou: false,
	phase: 'idle',
	problem: undefined,
	places: [],
	orders: [],
	keptTitle: '',
	nearTitle: '',
	keptMore: '',
	credits: [],
	nearProblem: undefined,
	nowUnixS: 0
};

export class TodayModel {
	state = $state.raw<TodayState>(NOTHING_YET);

	#core: CoreClient;
	/** The question last asked: an answer to an older one is let go. */
	#asked = 0;
	/** How often the pilot's own sites had changed when the take-offs were asked for. */
	#kept = 0;

	constructor(core: CoreClient) {
		this.#core = core;
	}

	/**
	 * Shows the take-offs for a place, unless they are shown already.
	 * `kept` counts the changes to the pilot's own sites: after one, the
	 * take-offs are asked for anew.
	 */
	show(place: Coordinate, nearYou: boolean, kept = 0): void {
		untrack(() => {
			const now = this.state;
			const same =
				now.place.lat === place.lat && now.place.lon === place.lon && kept === this.#kept;
			if (same && (now.phase === 'loading' || now.phase === 'loaded')) return;
			this.#kept = kept;
			void this.#load(place, nearYou);
		});
	}

	/** Asks again: after a failure. */
	again(): void {
		void this.#load(this.state.place, this.state.nearYou);
	}

	async #load(place: Coordinate, nearYou: boolean): Promise<void> {
		const asked = ++this.#asked;
		this.state = { ...NOTHING_YET, place, nearYou, phase: 'loading' };
		try {
			const found = await this.#core.todayTakeoffs(place);
			if (asked !== this.#asked) return;
			if (!found) {
				this.state = { ...this.state, phase: 'absent' };
				return;
			}
			const { listed } = found;
			this.state = {
				...this.state,
				phase: 'loaded',
				nowUnixS: found.nowUnixS,
				keptTitle: listed.keptTitle,
				nearTitle: listed.nearTitle,
				keptMore: listed.keptMore,
				credits: listed.credits,
				nearProblem: found.nearProblem,
				places: listed.takeoffs.map((takeoff) => ({
					...takeoff,
					outlook: undefined,
					problem: undefined
				}))
			};
			void this.#order(asked);
			await Promise.all(
				listed.takeoffs.map(async ({ site }) => {
					let outlook: SiteOutlook | undefined;
					let problem: string | undefined;
					try {
						outlook = await this.#core.todayOutlook(site.id, found.nowUnixS);
					} catch (error) {
						problem = words(error);
					}
					if (asked !== this.#asked) return;
					this.state = {
						...this.state,
						places: this.state.places.map((place) =>
							place.site.id === site.id ? { ...place, outlook, problem } : place
						)
					};
					void this.#order(asked);
				})
			);
		} catch (error) {
			if (asked === this.#asked) {
				this.state = { ...this.state, phase: 'failed', problem: words(error) };
			}
		}
	}

	/**
	 * Asks the core for the order of the take-offs as they stand now, on
	 * every day. An answer about take-offs that have changed since is let
	 * go: the change asked again.
	 */
	async #order(asked: number): Promise<void> {
		const places = this.state.places;
		try {
			const orders = await this.#core.todayOrders(standings(places));
			if (asked !== this.#asked || this.state.places !== places) return;
			this.state = { ...this.state, orders };
		} catch {
			// The take-offs stay as the core listed them.
		}
	}
}

function words(error: unknown): string {
	const said = error instanceof Error ? error.message : String(error);
	return isInWords(error) ? said : `Today’s forecast could not be fetched. ${said}`;
}
