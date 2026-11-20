/**
 * Today as the screen sees it: the take-offs around a place, and what the
 * forecast says about flying from each, as the iPhone's `TodayModel`.
 *
 * The take-offs are listed at once, and each forecast fills in as it
 * arrives. The place is the middle of the first region until the pilot asks
 * for their own position; nothing asks the browser before that.
 */

import { untrack } from 'svelte';
import type { Coordinate, CoreClient, OutlookDay, SiteOutlook, SiteView } from '#lib/core/index.ts';
import { isInWords } from '#lib/core/index.ts';

/** The middle of the first region, around the Tegernsee and the Brauneck. */
export const FIRST_REGION: Coordinate = { lat: 47.6, lon: 11.75 };

/** A take-off and what the forecast says of it. */
export interface TodayPlace {
	site: SiteView;
	distanceM: number;
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
	/** Now by Venti's clock, seconds since 1970-01-01 UTC, as of the last answer. */
	nowUnixS: number;
}

export const NOTHING_YET: TodayState = {
	place: FIRST_REGION,
	nearYou: false,
	phase: 'idle',
	problem: undefined,
	places: [],
	nowUnixS: 0
};

/** How the verdicts sort: the best first. */
const RANK = { flyable: 0, marginal: 1, notFlyable: 2 } as const;

/**
 * The take-off that looks best on a day: the best verdict, and of those the
 * one listed first. Nothing until one has a forecast for that day.
 */
export function bestOn(
	places: readonly TodayPlace[],
	day: number
): { place: TodayPlace; day: OutlookDay } | undefined {
	let best: { place: TodayPlace; day: OutlookDay } | undefined;
	for (const place of places) {
		const found = place.outlook?.days[day];
		if (found && (!best || RANK[found.verdict] < RANK[best.day.verdict])) {
			best = { place, day: found };
		}
	}
	return best;
}

/**
 * The day the screen opens on: today, unless today has no flying hours
 * left, which in the evening it has not; then tomorrow.
 */
export function firstDay(places: readonly TodayPlace[]): number {
	const withOutlook = places.filter((place) => place.outlook);
	if (withOutlook.length === 0) return 0;
	const hoursLeft = withOutlook.some((place) => (place.outlook!.days[0]?.hours.length ?? 0) > 0);
	return hoursLeft ? 0 : 1;
}

export class TodayModel {
	state = $state.raw<TodayState>(NOTHING_YET);

	#core: CoreClient;
	/** The question last asked: an answer to an older one is let go. */
	#asked = 0;

	constructor(core: CoreClient) {
		this.#core = core;
	}

	/** Shows the take-offs around a place, unless they are shown already. */
	show(place: Coordinate, nearYou: boolean): void {
		untrack(() => {
			const now = this.state;
			const same = now.place.lat === place.lat && now.place.lon === place.lon;
			if (same && (now.phase === 'loading' || now.phase === 'loaded')) return;
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
			this.state = {
				...this.state,
				phase: 'loaded',
				nowUnixS: found.nowUnixS,
				places: found.takeoffs.map((takeoff) => ({
					...takeoff,
					outlook: undefined,
					problem: undefined
				}))
			};
			await Promise.all(
				found.takeoffs.map(async ({ site }) => {
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
				})
			);
		} catch (error) {
			if (asked === this.#asked) {
				this.state = { ...this.state, phase: 'failed', problem: words(error) };
			}
		}
	}
}

function words(error: unknown): string {
	const said = error instanceof Error ? error.message : String(error);
	return isInWords(error) ? said : `Today’s forecast could not be fetched. ${said}`;
}
