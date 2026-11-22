/**
 * Foehn, as the worker fetches it: the server's reading of the pressure on
 * both sides of the Alps' main ridge, in the core's words for Today's one
 * line and for the view behind it.
 *
 * The path, the rule and every word are the core's
 * (`crates/core/src/foehn.rs`). This file asks the server and hands the
 * answer to the core with the places of the take-offs Today shows and the
 * time by the server's clock: 0 while the app has not read it, never this
 * device's own, and the core's words then carry the reading's own time.
 */

import { type Coordinate, foehn, foehnPath, type FoehnView } from './pkg/venti_core_wasm.js';
import { unreachable } from './protocol';
import { type Get, getFromThisOrigin } from './wind-source';

const getFoehn: Get = async (path) => {
	try {
		return await getFromThisOrigin(path);
	} catch {
		throw unreachable('Venti could not be reached.');
	}
};

export class FoehnSource {
	#present: () => Promise<boolean>;
	#now: () => Promise<number>;
	#get: Get;

	constructor(
		present: () => Promise<boolean> = async () => true,
		now: () => Promise<number> = async () => 0,
		get: Get = getFoehn
	) {
		this.#present = present;
		this.#now = now;
		this.#get = get;
	}

	/**
	 * The foehn view, with the line for Today where a pair of stations near
	 * one of `places` has something to say. Nothing if this site has no
	 * server. Throws in words when the server gives no usable answer.
	 */
	async read(places: Coordinate[]): Promise<FoehnView | undefined> {
		if (!(await this.#present())) return undefined;
		const answer = await this.#get(foehnPath());
		return foehn(answer.status, answer.body, places, await this.#now());
	}
}
