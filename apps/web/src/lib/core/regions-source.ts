/**
 * The thermal forecast by region, as the worker fetches it: the regions'
 * outlines once, and a day's level for each region, for the day and hour
 * by hour.
 *
 * What to ask and how to read the answers are the core's (`regionsPath`,
 * `ForecastRegions`, `regionForecastPath`, `RegionForecast`): the names,
 * the levels, every word and the credits. This file sends the requests in
 * between, tells the core the moment by Venti's clock and how far the
 * pilot's clock is ahead of UTC, and keeps the outlines while the
 * forecast names the same set of regions.
 */

import {
	type Credit,
	ForecastRegions,
	RegionForecast,
	regionForecastPath,
	type RegionForecastView,
	type RegionLevelKey,
	regionLevels,
	type RegionReadingView,
	regionsPath
} from './pkg/venti_core_wasm.js';
import { unreachable } from './protocol';
import { type Get, getFromThisOrigin, utcOffsetAt } from './wind-source';

/** The regions as a map draws them. */
export interface RegionOutlines {
	version: string;
	/** GeoJSON text: each outline with `id` and `name`. */
	outlines: string;
	credits: Credit[];
	/** The key, the best level first. */
	levels: RegionLevelKey[];
}

/** A day of the forecast by region, read whole: the day and each of its hours. */
export interface RegionDay {
	view: RegionForecastView;
	day: RegionReadingView[];
	/** By the hour's number in the view's `hours`. */
	hours: RegionReadingView[][];
}

/** The shell's words for a server that gave no answer at all. */
export const REGIONS_UNREACHABLE = 'Venti could not be reached for the forecast by region.';

/** The language the regions are named in: the app's. */
const LANGUAGE = 'en';

const getRegions: Get = async (path) => {
	try {
		return await getFromThisOrigin(path);
	} catch {
		throw unreachable(REGIONS_UNREACHABLE);
	}
};

export class RegionsSource {
	#present: () => Promise<boolean>;
	#now: () => Promise<number>;
	#get: Get;
	/** The outlines read last, which a forecast of the same set of regions keeps. */
	#outlines: RegionOutlines | undefined;

	constructor(
		present: () => Promise<boolean> = async () => true,
		now: () => Promise<number> = async () => Date.now() / 1000,
		get: Get = getRegions
	) {
		this.#present = present;
		this.#now = now;
		this.#get = get;
	}

	/**
	 * The regions, asked of the server once, and again when a forecast
	 * names another set of them (`version`). Nothing if this site has no
	 * server.
	 */
	async outlines(version?: string): Promise<RegionOutlines | undefined> {
		if (!(await this.#present())) return undefined;
		if (this.#outlines && (version === undefined || this.#outlines.version === version)) {
			return this.#outlines;
		}
		const answer = await this.#get(regionsPath());
		const regions = new ForecastRegions(answer.status, answer.body, LANGUAGE);
		try {
			this.#outlines = {
				version: regions.version(),
				outlines: regions.outlines(),
				credits: regions.credits(),
				levels: regionLevels()
			};
		} finally {
			regions.free();
		}
		return this.#outlines;
	}

	/** A day of the forecast by region, by its date as the thermal map names it. Nothing without a server. */
	async day(date: string): Promise<RegionDay | undefined> {
		if (!(await this.#present())) return undefined;
		const answer = await this.#get(regionForecastPath(date));
		const nowUnixS = await this.#now();
		const forecast = new RegionForecast(
			answer.status,
			answer.body,
			utcOffsetAt(nowUnixS),
			nowUnixS
		);
		try {
			const view = forecast.view();
			return {
				view,
				day: forecast.day(),
				hours: view.hours.map((_, hour) => forecast.hour(hour))
			};
		} finally {
			forecast.free();
		}
	}
}
