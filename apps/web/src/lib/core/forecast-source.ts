/**
 * The forecast for a place from the Venti server, as the worker fetches it:
 * what the server's weather models compute for it, hour by hour.
 *
 * The core says what to ask the server for and reads its answer. This file
 * sends the request in between and says by which clock the forecast's days
 * are cut: this browser's. Nothing is kept: the browser's own cache reuses
 * an answer for as long as the server allows.
 *
 * The server is the one this site is served from, as for the wind.
 */

import {
	type Coordinate,
	forecast,
	forecastPath,
	type ForecastView
} from './pkg/venti_core_wasm.js';
import { unreachable } from './protocol';
import { type Get, getFromThisOrigin } from './wind-source';

/** Fetches from this site's origin, and says what it was for when no answer comes. */
const getForecast: Get = async (path) => {
	try {
		return await getFromThisOrigin(path);
	} catch {
		throw unreachable('The server could not be reached for the forecast.');
	}
};

/** How far this browser's clock is ahead of UTC right now, in seconds. */
const browsersOffsetS = () => -new Date().getTimezoneOffset() * 60;

export class ForecastSource {
	/** Whether this site has a server at all. One without is never asked. */
	#present: () => Promise<boolean>;
	#get: Get;
	#utcOffsetS: () => number;

	constructor(
		present: () => Promise<boolean> = async () => true,
		get: Get = getForecast,
		utcOffsetS: () => number = browsersOffsetS
	) {
		this.#present = present;
		this.#get = get;
		this.#utcOffsetS = utcOffsetS;
	}

	/**
	 * What the weather models say about a place. Nothing if this site has
	 * no server. Throws, with words for the pilot, when the server gives no
	 * forecast: it has none, the place is outside its area, or it is busy.
	 */
	async forecast(place: Coordinate): Promise<ForecastView | undefined> {
		if (!(await this.#present())) return undefined;
		const answer = await this.#get(forecastPath(place.lat, place.lon));
		return forecast(answer.status, answer.body, this.#utcOffsetS());
	}
}
