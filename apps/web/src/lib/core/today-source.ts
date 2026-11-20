/**
 * Today, as the worker fetches it: the take-offs around a place, and what
 * the forecast says about flying from each.
 *
 * Where the take-offs are and what a forecast says of one are the core's
 * (`sitesMap`, `siteOutlook`); this file asks for the sites around the
 * place, keeps the nearest take-offs, and asks for the forecast of each,
 * again while the server answers that it is still fetching it, as the
 * iPhone's `TodayModel` does. Nothing is kept beyond the take-offs last
 * listed, by identifier, so that the page can ask for each one's forecast.
 */

import {
	type Coordinate,
	forecastPath,
	siteOutlook,
	type SiteOutlook,
	sitesMap,
	sitesPath,
	type SiteView
} from './pkg/venti_core_wasm.js';
import { unreachable } from './protocol';
import { type Get, getFromThisOrigin } from './wind-source';

/** How many take-offs are shown: the nearest ones. */
export const TAKEOFFS_SHOWN = 6;
/** How far from the place a take-off may lie, metres. */
export const REACH_M = 30_000;
/** How often a forecast the server is still fetching is asked for. */
const FORECAST_TRIES = 7;

/** A take-off near the place asked about. */
export interface TodayTakeoff {
	site: SiteView;
	/** How far it is from the place asked about, metres. */
	distanceM: number;
}

/** The take-offs around a place, and the moment by Venti's clock they are seen at. */
export interface TodayTakeoffs {
	takeoffs: TodayTakeoff[];
	/** Now, by Venti's clock, seconds since 1970-01-01 UTC. */
	nowUnixS: number;
}

/** Metres between two places along the earth's surface. */
export function metresBetween(one: Coordinate, other: Coordinate): number {
	const radians = Math.PI / 180;
	const [lat1, lat2] = [one.lat * radians, other.lat * radians];
	const a =
		Math.sin((lat2 - lat1) / 2) ** 2 +
		Math.cos(lat1) * Math.cos(lat2) * Math.sin(((other.lon - one.lon) * radians) / 2) ** 2;
	return 2 * 6_371_000 * Math.asin(Math.min(1, Math.sqrt(a)));
}

/**
 * The take-offs to show of those a map holds: within reach of the place,
 * not marked as discouraged, those with a name of their own first ("Take-
 * off" tells a pilot nothing), and among them and the rest the nearest.
 */
export function nearestTakeoffs(sites: readonly SiteView[], place: Coordinate): TodayTakeoff[] {
	return sites
		.filter((site) => site.uses.includes('takeoff') && !site.discouraged)
		.map((site) => ({ site, distanceM: metresBetween(place, site.position) }))
		.filter((takeoff) => takeoff.distanceM <= REACH_M)
		.toSorted((one, other) =>
			one.site.isNamed !== other.site.isNamed
				? one.site.isNamed
					? -1
					: 1
				: one.distanceM - other.distanceM
		)
		.slice(0, TAKEOFFS_SHOWN);
}

const getToday: Get = async (path) => {
	try {
		return await getFromThisOrigin(path);
	} catch {
		throw unreachable('Venti could not be reached for today’s forecast.');
	}
};

export class TodaySource {
	#present: () => Promise<boolean>;
	#now: () => Promise<number>;
	#get: Get;
	#pauseMs: number;
	/** The take-offs last listed, by identifier. */
	#sites = new Map<string, SiteView>();

	constructor(
		present: () => Promise<boolean> = async () => true,
		now: () => Promise<number> = async () => Date.now() / 1000,
		get: Get = getToday,
		pauseMs = 4000
	) {
		this.#present = present;
		this.#now = now;
		this.#get = get;
		this.#pauseMs = pauseMs;
	}

	/** The take-offs around a place. Nothing if this site has no server. */
	async takeoffs(place: Coordinate): Promise<TodayTakeoffs | undefined> {
		if (!(await this.#present())) return undefined;
		// A box that holds the circle of the reach.
		const north = REACH_M / 111_195;
		const east = north / Math.max(Math.cos((place.lat * Math.PI) / 180), 0.2);
		const [south, west, top, right] = [
			place.lat - north,
			place.lon - east,
			place.lat + north,
			place.lon + east
		];
		const answer = await this.#get(sitesPath(south, west, top, right));
		const map = sitesMap(south, west, top, right, answer.status, answer.body);
		const takeoffs = nearestTakeoffs(map.sites, place);
		this.#sites = new Map(takeoffs.map((takeoff) => [takeoff.site.id, takeoff.site]));
		return { takeoffs, nowUnixS: await this.#now() };
	}

	/**
	 * What the forecast says about flying from one of the take-offs last
	 * listed, from `nowUnixS` on. A forecast the server is still fetching is
	 * asked for again a few times. Throws in words when there is none.
	 */
	async outlook(id: string, nowUnixS: number): Promise<SiteOutlook> {
		const site = this.#sites.get(id);
		if (!site) throw unreachable('This take-off is no longer listed.');
		const path = forecastPath(site.position.lat, site.position.lon);
		for (let tries = 1; ; tries += 1) {
			const answer = await this.#get(path);
			if (answer.status === 503 && tries < FORECAST_TRIES) {
				await new Promise((done) => setTimeout(done, this.#pauseMs));
				continue;
			}
			const offsetS = -new Date(nowUnixS * 1000).getTimezoneOffset() * 60;
			return siteOutlook(answer.status, answer.body, offsetS, site.wind, site.elevationM, nowUnixS);
		}
	}
}
