/**
 * Today, as the worker fetches it: the take-offs the core lists, and what
 * the forecast says about flying from each.
 *
 * Which take-offs Today lists, in which order, and what a forecast says of
 * one are the core's (`todayArea`, `Core.todayTakeoffs`, `todayOrder`,
 * `siteOutlook`): the pilot's own first, then those around the place. This
 * file sends the request for the sites around the place in between, and
 * asks for the forecast of each take-off, again while the server answers
 * that it is still fetching it, as the iPhone's `TodayModel` does. Nothing
 * is kept here beyond the take-offs last listed, by identifier, so that the
 * page can ask for each one's forecast.
 */

import {
	type Coordinate,
	forecastPath,
	siteOutlook,
	type SiteOutlook,
	sitesMap,
	type SitesMap,
	sitesPath,
	type SiteView,
	todayArea,
	todayOrder,
	type TodayOrder,
	type TodayStanding,
	type TodayTakeoffs
} from './pkg/venti_core_wasm.js';
import { unreachable } from './protocol';
import type { CoreSession } from './session';
import { type Get, getFromThisOrigin, utcOffsetAt } from './wind-source';

/** How often a forecast the server is still fetching is asked for. */
const FORECAST_TRIES = 7;

/** Today's take-offs as the core lists them, and the moment by Venti's clock they are seen at. */
export interface TodayAnswer {
	/** The take-offs, their two headings and their credits: the core's. */
	listed: TodayTakeoffs;
	/** Now, by Venti's clock, seconds since 1970-01-01 UTC. */
	nowUnixS: number;
	/**
	 * Why Venti gave no take-offs around the place, in words, when the
	 * pilot's own are listed all the same.
	 */
	nearProblem: string | undefined;
}

/**
 * The take-offs in order on each of several days: the core's order, asked
 * for once for every day the screen can show.
 */
export function todayOrders(days: readonly TodayStanding[][]): TodayOrder[] {
	return days.map((standings) => todayOrder(standings));
}

const getToday: Get = async (path) => {
	try {
		return await getFromThisOrigin(path);
	} catch {
		throw unreachable('Venti could not be reached for today’s forecast.');
	}
};

export class TodaySource {
	#session: CoreSession;
	#present: () => Promise<boolean>;
	#now: () => Promise<number>;
	#get: Get;
	#pauseMs: number;
	/** The take-offs last listed, by identifier. */
	#sites = new Map<string, SiteView>();

	constructor(
		session: CoreSession,
		present: () => Promise<boolean> = async () => true,
		now: () => Promise<number> = async () => Date.now() / 1000,
		get: Get = getToday,
		pauseMs = 4000
	) {
		this.#session = session;
		this.#present = present;
		this.#now = now;
		this.#get = get;
		this.#pauseMs = pauseMs;
	}

	/**
	 * The take-offs Today lists for a place: the pilot's own, then those
	 * around it. Nothing if this site has no server.
	 *
	 * When Venti gives no answer about the sites around, the pilot's own are
	 * listed alone, with why; with none of their own, that is the failure.
	 * The core brings a kept site up to date from the answer, which is a
	 * change to what is stored.
	 */
	async takeoffs(place: Coordinate): Promise<TodayAnswer | undefined> {
		if (!(await this.#present())) return undefined;
		const { south, west, north, east } = todayArea(place.lat, place.lon);
		let near: SitesMap | undefined;
		let failure: unknown;
		try {
			const answer = await this.#get(sitesPath(south, west, north, east));
			near = sitesMap(south, west, north, east, answer.status, answer.body);
		} catch (error) {
			failure = error;
		}
		const listed = await this.#session.change((core) => core.todayTakeoffs(place, near));
		if (failure !== undefined && listed.takeoffs.length === 0) throw failure;
		this.#sites = new Map(listed.takeoffs.map((takeoff) => [takeoff.site.id, takeoff.site]));
		return {
			listed,
			nowUnixS: await this.#now(),
			nearProblem:
				failure === undefined
					? undefined
					: failure instanceof Error
						? failure.message
						: String(failure)
		};
	}

	/**
	 * What the forecast says about flying from a site, by Venti's clock now:
	 * for a site's page on the map. Nothing if this site has no server.
	 */
	async outlookFor(
		site: Pick<SiteView, 'position' | 'wind' | 'elevationM'>
	): Promise<SiteOutlook | undefined> {
		if (!(await this.#present())) return undefined;
		return this.#read(site, await this.#now());
	}

	/**
	 * What the forecast says about flying from one of the take-offs last
	 * listed, from `nowUnixS` on. A forecast the server is still fetching is
	 * asked for again a few times. Throws in words when there is none.
	 */
	async outlook(id: string, nowUnixS: number): Promise<SiteOutlook> {
		const site = this.#sites.get(id);
		if (!site) throw unreachable('This take-off is no longer listed.');
		return this.#read(site, nowUnixS);
	}

	async #read(
		site: Pick<SiteView, 'position' | 'wind' | 'elevationM'>,
		nowUnixS: number
	): Promise<SiteOutlook> {
		const path = forecastPath(site.position.lat, site.position.lon);
		for (let tries = 1; ; tries += 1) {
			const answer = await this.#get(path);
			if (answer.status === 503 && tries < FORECAST_TRIES) {
				await new Promise((done) => setTimeout(done, this.#pauseMs));
				continue;
			}
			const offsetS = utcOffsetAt(nowUnixS);
			return siteOutlook(answer.status, answer.body, offsetS, site.wind, site.elevationM, nowUnixS);
		}
	}
}
