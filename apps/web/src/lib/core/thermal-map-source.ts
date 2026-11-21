/**
 * The thermal map, as the worker fetches it: the kind of day, square by
 * square, over the area the Venti server has forecasts for.
 *
 * What to ask and how to read the answer are the core's (`thermalMapPath`,
 * `thermalMap`): which day is today, each square's step of the thermal
 * scale and every word. This file sends the request in between and tells
 * the core the moment by Venti's clock and how far the pilot's clock is
 * ahead of UTC. Nothing is kept here; the page keeps the newest map.
 */

import { thermalMap, type ThermalMap, thermalMapPath } from './pkg/venti_core_wasm.js';
import { unreachable } from './protocol';
import { type Get, getFromThisOrigin } from './wind-source';

/** The shell's words for a server that gave no answer at all. */
export const THERMAL_MAP_UNREACHABLE = 'Venti could not be reached for the forecast map.';

const getMap: Get = async (path) => {
	try {
		return await getFromThisOrigin(path);
	} catch {
		throw unreachable(THERMAL_MAP_UNREACHABLE);
	}
};

export class ThermalMapSource {
	#present: () => Promise<boolean>;
	#now: () => Promise<number>;
	#get: Get;

	constructor(
		present: () => Promise<boolean> = async () => true,
		now: () => Promise<number> = async () => Date.now() / 1000,
		get: Get = getMap
	) {
		this.#present = present;
		this.#now = now;
		this.#get = get;
	}

	/**
	 * The map from today on, by Venti's clock and the pilot's days. Nothing
	 * if this site has no server. Throws, in words for the pilot, when the
	 * server gives no usable answer.
	 */
	async map(): Promise<ThermalMap | undefined> {
		if (!(await this.#present())) return undefined;
		const answer = await this.#get(thermalMapPath());
		const nowUnixS = await this.#now();
		const offsetS = -new Date(nowUnixS * 1000).getTimezoneOffset() * 60;
		return thermalMap(answer.status, answer.body, offsetS, nowUnixS);
	}
}
