/**
 * Keeps the flying sites for the part of the map in view: when to ask the
 * server, how long an answer is used, and what the screens are shown. The
 * iPhone app's `SitesModel.swift` follows the same rules, with one
 * difference. It tells a valley from a country by the width of its map in
 * degrees, which on a phone stands for a zoom; a browser's window can be
 * any width, so this file goes by the zoom itself.
 *
 * What to ask and how to read the answer is the core's, in the worker. This
 * file knows nothing about the page: `model.svelte.ts` connects it to the
 * screens.
 */

import type { Bounds, Credit, SitesMap, SiteView } from '#lib/core/index.ts';
import { isInWords } from '#lib/core/index.ts';
import { contains, grown } from '#lib/wind/watch.ts';

/**
 * From this zoom on the map shows a valley and not a country: sites are
 * asked for and drawn. Further out they would be a carpet of dots.
 */
export const SITES_MIN_ZOOM = 9;

/**
 * How long an answer is used before the server is asked again, in
 * milliseconds. Sites change by the day, not by the minute.
 */
export const KEPT_MS = 60 * 60_000;

/** How much more than the part in view is asked for, so that a drag needs no new request. */
const GROWN_BY = 0.5;

/** What the screens are shown. */
export interface SitesState {
	/** Whether the map is close enough for sites to be drawn and listed. */
	close: boolean;
	/** Whether the server has answered for the part of the map in view. */
	known: boolean;
	/** The sites around the part of the map in view. None while the map is too far out. */
	sites: SiteView[];
	/** Whose data they are. The licences ask for this next to the data. */
	credits: Credit[];
	/** The date of the oldest source's data, seconds since 1970-01-01 UTC. */
	asOfUnixS: number | undefined;
	/**
	 * Whether the server holds everything its sources have for the area.
	 * If not, a map without sites says nothing.
	 */
	isWhole: boolean;
	/** Why the sites are missing, in words for the pilot. */
	problem: string | undefined;
}

export const NO_SITES: SitesState = {
	close: false,
	known: false,
	sites: [],
	credits: [],
	asOfUnixS: undefined,
	isWhole: false,
	problem: undefined
};

export class SitesWatch {
	#ask: (box: Bounds) => Promise<SitesMap | undefined>;
	#tell: (state: SitesState) => void;
	#now: () => number;
	#state = NO_SITES;
	/** The part of the map in view, while the map is close enough. */
	#wanted: Bounds | undefined;
	/** The last answer, the area it was asked for, and when it came. */
	#answered: { map: SitesMap; area: Bounds; at: number } | undefined;
	/** Why the last request failed, if it did. */
	#problem: string | undefined;
	/** Whether the site turned out to have no server. */
	#absent = false;
	/** Counts the requests, so that an answer overtaken by a later one is dropped. */
	#asked = 0;

	constructor(
		ask: (box: Bounds) => Promise<SitesMap | undefined>,
		tell: (state: SitesState) => void,
		now: () => number = Date.now
	) {
		this.#ask = ask;
		this.#tell = tell;
		this.#now = now;
	}

	/**
	 * The map says which part of it is in view, and how close it is.
	 * Returns whether the server has to be asked: not when the map shows too
	 * much for sites to be drawn, and not when that part is covered by an
	 * answer still in use.
	 */
	show(region: Bounds, zoom: number): boolean {
		if (zoom < SITES_MIN_ZOOM) {
			// An answer still on its way is kept for when the map comes back.
			this.#wanted = undefined;
			this.#show();
			return false;
		}
		this.#wanted = region;
		this.#show();
		return !this.#absent && !this.#covers(region);
	}

	/** Asks the server for the sites around the part of the map in view. */
	async load(): Promise<void> {
		const region = this.#wanted;
		// The map may have moved on, or been answered for, since it was told.
		if (!region || this.#absent || this.#covers(region)) return;
		const area = grown(region, GROWN_BY);
		const asked = ++this.#asked;
		try {
			const map = await this.#ask(area);
			if (asked !== this.#asked) return;
			if (!map) {
				// A site without a server has a map without sites, and says
				// nothing about it.
				this.#absent = true;
				this.#answered = undefined;
			} else {
				this.#answered = { map, area, at: this.#now() };
			}
			this.#problem = undefined;
		} catch (error) {
			if (asked !== this.#asked) return;
			// What was drawn stays: the sites have not moved.
			this.#problem = words(error);
		}
		this.#show();
	}

	/**
	 * Lays a site the pilot has just added, corrected or put back over what
	 * the server last answered, so that the map shows it without waiting
	 * for the server to be asked again.
	 */
	put(site: SiteView): void {
		this.#lay((sites) => [...sites.filter((one) => one.id !== site.id), site]);
	}

	/** Takes a site the pilot has just removed out of what the server last answered. */
	drop(id: string): void {
		this.#lay((sites) => sites.filter((one) => one.id !== id));
	}

	#lay(change: (sites: SiteView[]) => SiteView[]): void {
		const answered = this.#answered;
		if (!answered) return;
		this.#answered = { ...answered, map: { ...answered.map, sites: change(answered.map.sites) } };
		this.#show();
	}

	/** Whether an answer still in use holds the sites of all of `region`. */
	#covers(region: Bounds): boolean {
		const answered = this.#answered;
		return Boolean(
			answered && contains(answered.area, region) && this.#now() - answered.at < KEPT_MS
		);
	}

	#show(): void {
		const close = this.#wanted !== undefined;
		const map = close ? this.#answered?.map : undefined;
		const state: SitesState = {
			close,
			known: this.#wanted !== undefined && this.#covers(this.#wanted),
			sites: map?.sites ?? NO_SITES.sites,
			credits: map?.credits ?? NO_SITES.credits,
			asOfUnixS: map?.asOfUnixS,
			isWhole: map?.isWhole ?? false,
			problem: close ? this.#problem : undefined
		};
		if (same(state, this.#state)) return;
		this.#state = state;
		this.#tell(state);
	}
}

/** Whether two states show the screens the same thing. */
function same(first: SitesState, second: SitesState): boolean {
	return (
		first.close === second.close &&
		first.known === second.known &&
		first.sites === second.sites &&
		first.credits === second.credits &&
		first.asOfUnixS === second.asOfUnixS &&
		first.isWhole === second.isWhole &&
		first.problem === second.problem
	);
}

/** Why the sites could not be fetched, for the pilot. */
function words(error: unknown): string {
	const said = error instanceof Error ? error.message : String(error);
	return isInWords(error) ? said : `The flying sites could not be shown. ${said}`;
}
