/**
 * The weather along the route being planned: the pilot's start, mean speed
 * over the ground and glide, and what the core says of the route with the
 * forecast of a day of the thermal map. The speed and the glide are kept in
 * the browser, like the map's other choices; the start is today's next full
 * hour until the pilot says otherwise.
 */

import type { RouteAlong, RouteWeatherAnswer, RouteWeatherAsked } from '#lib/core/index.ts';
import type { RoutePlace } from './kept';

/** Asks the core for the route with the weather along it. */
export type AskWeather = (asked: RouteWeatherAsked) => Promise<RouteWeatherAnswer | undefined>;

type Storage = Pick<globalThis.Storage, 'getItem' | 'setItem'> | undefined;

const KEY = 'venti.route-setting';

/** What the planner starts with: 20 km/h over the ground, climbs included, and a glide of 8. */
export const DEFAULT_SPEED_KMH = 20;
export const DEFAULT_GLIDE = 8;

const two = (value: number) => String(value).padStart(2, '0');

/** The next full hour after a moment, on the browser's clock: "14:00". */
export function nextFullHour(now: Date): string {
	return `${two((now.getHours() + 1) % 24)}:00`;
}

/** A day on the browser's clock as the thermal map writes its days: "2026-11-22". */
export function dateOf(now: Date): string {
	return `${now.getFullYear()}-${two(now.getMonth() + 1)}-${two(now.getDate())}`;
}

/** A day and a time of day on the browser's clock, in Unix seconds. */
export function startUnixS(date: string, time: string): number {
	return Math.round(new Date(`${date}T${time}:00`).getTime() / 1000);
}

/** The speed and the glide this browser keeps; the defaults where it keeps none. */
export function keptSetting(storage: Storage): { speedKmh: number; glide: number } {
	try {
		const kept: unknown = JSON.parse(storage?.getItem(KEY) ?? 'null');
		const { speedKmh, glide } = (kept ?? {}) as Record<string, unknown>;
		return {
			speedKmh: typeof speedKmh === 'number' && speedKmh > 0 ? speedKmh : DEFAULT_SPEED_KMH,
			glide: typeof glide === 'number' && glide > 0 ? glide : DEFAULT_GLIDE
		};
	} catch {
		return { speedKmh: DEFAULT_SPEED_KMH, glide: DEFAULT_GLIDE };
	}
}

export class RouteWeatherModel {
	/** The day of the thermal map the route is flown on, by its date; the map's first without one. */
	date = $state<string>();
	/** When the pilot leaves the first place, on the browser's clock: "14:00". */
	time = $state('');
	/** The mean speed over the ground, climbs included, km/h. */
	speedKmh = $state(DEFAULT_SPEED_KMH);
	/** The glide ratio in still air. */
	glide = $state(DEFAULT_GLIDE);
	/** Whether the core is being asked, with no answer yet. */
	asking = $state(false);
	#last?: { places: readonly RoutePlace[]; days: readonly { date: string }[] };
	/** What the core says of the route, once it has answered. */
	along = $state.raw<RouteAlong>();
	/** Why there is no forecast along it, or no answer at all, in words. */
	problem = $state.raw<string>();

	#storage: Storage;
	#ask: AskWeather;
	#now: () => Date;
	#asked = 0;

	constructor(storage: Storage, ask: AskWeather, now: () => Date = () => new Date()) {
		this.#storage = storage;
		this.#ask = ask;
		this.#now = now;
		const kept = keptSetting(storage);
		this.speedKmh = kept.speedKmh;
		this.glide = kept.glide;
		this.time = nextFullHour(now());
	}

	/** Keeps the speed and the glide for the next visit. */
	keep(): void {
		try {
			this.#storage?.setItem(KEY, JSON.stringify({ speedKmh: this.speedKmh, glide: this.glide }));
		} catch {
			// A private window, or no storage: the setting lasts for the visit.
		}
	}

	/**
	 * Asks the core about a route on one of the thermal map's days, the
	 * chosen one or else the first; without days, today without a forecast.
	 * An answer for a route or a setting that has changed since is dropped.
	 */
	async show(places: readonly RoutePlace[], days: readonly { date: string }[] = []): Promise<void> {
		const mine = ++this.#asked;
		this.#last = { places, days };
		const day = days.find((one) => one.date === this.date) ?? days[0];
		const date = day?.date ?? dateOf(this.#now());
		const asked: RouteWeatherAsked = {
			setting: {
				places: places.map(({ lat, lon, name }) => ({ lat, lon, name })),
				startUnixS: startUnixS(date, this.time || nextFullHour(this.#now())),
				groundSpeedKmh: this.speedKmh,
				glideRatio: this.glide,
				flyingHeightM: undefined
			},
			date: day?.date
		};
		if (places.length < 2) {
			this.along = undefined;
			this.problem = undefined;
			this.asking = false;
			return;
		}
		this.asking = true;
		try {
			const answer = await this.#ask(asked);
			if (mine !== this.#asked) return;
			this.along = answer?.along;
			this.problem = answer?.problem;
		} catch (error) {
			if (mine !== this.#asked) return;
			this.along = undefined;
			this.problem = error instanceof Error ? error.message : String(error);
		} finally {
			if (mine === this.#asked) this.asking = false;
		}
	}

	/** Asks again about the route last asked about, after a problem. */
	again(): Promise<void> {
		return this.#last ? this.show(this.#last.places, this.#last.days) : Promise.resolve();
	}
}
