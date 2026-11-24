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

import {
	type Coordinate,
	type ThermalHoursLayer,
	thermalHoursPath,
	thermalMap,
	type ThermalMap,
	ThermalMapHours,
	type ThermalMapHourSquare,
	type ThermalMapWind,
	type ThermalMapHoursView,
	thermalMapPath,
	type ThermalMapShapes,
	thermalMapStrip,
	type ThermalStripDay
} from './pkg/venti_core_wasm.js';
import { unreachable } from './protocol';
import { type Answer, type Get, getFromThisOrigin, utcOffsetAt } from './wind-source';

/** One layer of one day, at one level for the wind: what the hours are asked for by. */
export interface HoursAsked {
	date: string;
	layer: ThermalHoursLayer;
	/** The height of the wind above the sea; nothing at the ground. */
	heightM?: number;
	/** The height of the wind above the model's ground, for a level that is one: 250. */
	aboveGroundM?: number;
}

/** The name an answer of hours is kept under. */
export function hoursKey(asked: HoursAsked): string {
	const level =
		asked.heightM ?? (asked.aboveGroundM === undefined ? 'ground' : `${asked.aboveGroundM} agl`);
	return `${asked.date} ${asked.layer} ${level}`;
}

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
	/** The newest answer of the map of the days, which a square's days are read from again. */
	#days: Answer | undefined;
	/** The answers of hours read for that map, by `hoursKey`: objects of the core, freed with it. */
	#hours = new Map<string, ThermalMapHours>();
	/** The server's answers of hours, by path: the wind at the ground and the gusts share one. */
	#answers = new Map<string, Promise<Answer>>();

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
		const { nowUnixS, offsetS } = await this.#clock();
		const read = thermalMap(answer.status, answer.body, offsetS, nowUnixS);
		// Another map: the hours read for the one before are let go.
		if (!this.#days || !sameBytes(this.#days.body, answer.body)) this.#forget();
		this.#days = answer;
		return read;
	}

	/**
	 * One layer of one day hour by hour, as a card shows it: the hours, the
	 * one to show first, the model and its run. Asked of the server once
	 * for each day, layer and level while the map of the days is the same.
	 */
	async hours(asked: HoursAsked): Promise<ThermalMapHoursView> {
		const key = hoursKey(asked);
		let hours = this.#hours.get(key);
		if (!hours) {
			const path = thermalHoursPath(asked.date, asked.layer, asked.heightM, asked.aboveGroundM);
			let answer = this.#answers.get(path);
			if (!answer) {
				answer = this.#get(path);
				this.#answers.set(path, answer);
				// A failure is not kept: "Try again" asks again.
				answer.catch(() => this.#answers.delete(path));
			}
			const { status, body } = await answer;
			const { nowUnixS, offsetS } = await this.#clock();
			let read: ThermalMapHours;
			try {
				read = new ThermalMapHours(status, body, asked.layer, offsetS, nowUnixS);
			} catch (error) {
				// A refusal is not kept either.
				this.#answers.delete(path);
				throw error;
			}
			// Asked twice at once, the first answer read is kept and the other let go.
			hours = this.#hours.get(key);
			if (hours) read.free();
			else this.#hours.set(key, (hours = read));
		}
		return hours.view();
	}

	/** What the map draws of one hour: the squares and the arrows, as GeoJSON texts. */
	shapes(asked: HoursAsked, hour: number): ThermalMapShapes {
		return this.#held(asked).shapes(hour);
	}

	/** A square in one hour, in words; nothing where there is none. */
	square(asked: HoursAsked, cell: number, hour: number): ThermalMapHourSquare | undefined {
		return this.#held(asked).square(cell, hour);
	}

	/** The number of the square a place lies in, also where none is drawn. */
	cellAt(asked: HoursAsked, lat: number, lon: number): number | undefined {
		return this.#held(asked).cellAt(lat, lon);
	}

	/**
	 * The forecast wind at each place in one hour, as the core reads it of
	 * the square the place lies in (`windAt`): for the take-offs' fans.
	 * Nothing for a place outside the map or without a wind.
	 */
	windsAt(
		asked: HoursAsked,
		hour: number,
		places: readonly Coordinate[]
	): (ThermalMapWind | undefined)[] {
		const hours = this.#held(asked);
		return places.map((place) => hours.windAt(place.lat, place.lon, hour) ?? undefined);
	}

	/** Every day of the map at one square, side by side; none before the map was read. */
	async strip(lat: number, lon: number): Promise<ThermalStripDay[]> {
		if (!this.#days) return [];
		const { nowUnixS, offsetS } = await this.#clock();
		return thermalMapStrip(this.#days.status, this.#days.body, offsetS, nowUnixS, lat, lon);
	}

	#held(asked: HoursAsked): ThermalMapHours {
		const hours = this.#hours.get(hoursKey(asked));
		if (!hours) throw new Error('These hours were not read, or were let go.');
		return hours;
	}

	#forget(): void {
		for (const hours of this.#hours.values()) hours.free();
		this.#hours.clear();
		this.#answers.clear();
	}

	/** The moment by Venti's clock, and how far the pilot's clock is ahead of UTC then. */
	async #clock(): Promise<{ nowUnixS: number; offsetS: number }> {
		const nowUnixS = await this.#now();
		return { nowUnixS, offsetS: utcOffsetAt(nowUnixS) };
	}
}

function sameBytes(one: Uint8Array, other: Uint8Array): boolean {
	return one.length === other.length && one.every((byte, index) => byte === other[index]);
}
