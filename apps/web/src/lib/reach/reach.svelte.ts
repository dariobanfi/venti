/**
 * What a glide reaches from a place on the map, as the map tab holds it:
 * the place, the pilot's three settings (the glide, the start height and
 * the wind) and the core's answer for them. Every change asks the core
 * again, in the worker, over the ground it fetched for the place; the
 * server is not asked. An answer for a place or a setting changed since is
 * dropped.
 *
 * The glide starts as the one the route planner keeps in this browser, the
 * only glide the web app keeps (8 until the pilot sets one); the start
 * height as the ground, a start from a summit; the wind as none.
 */

import { messageOf } from '#lib/core/index.ts';
import type { Coordinate, GlideReach, GlideReachSetting } from '#lib/core/index.ts';
import { keptSetting } from '#lib/route/weather.svelte.ts';

/** Asks the core what a glide reaches. */
export type AskReach = (setting: GlideReachSetting) => Promise<GlideReach | undefined>;

type Storage = Parameters<typeof keptSetting>[0];

/** The steps of the settings: the glide by half a point, the height by 50 m. */
export const GLIDE_STEP = 0.5;
export const HEIGHT_STEP_M = 50;

/** The wind the switch starts with, until the pilot sets another. */
export const FIRST_WIND = { fromDeg: 270, kmh: 15 };

export class GlideReachModel {
	/** The place the glide starts from, while the area is shown. */
	place = $state.raw<Coordinate>();
	/** The glide ratio in still air. */
	glide = $state(8);
	/** The height to start at, metres; none for the ground at the place. */
	startHeightM = $state<number>();
	/** Whether the glide is laid in a wind. */
	windOn = $state(false);
	/** Where the wind comes from, degrees, and how fast it blows, km/h. */
	windFromDeg = $state(FIRST_WIND.fromDeg);
	windKmh = $state(FIRST_WIND.kmh);
	/** What the core says, once it has answered for the place. */
	reach = $state.raw<GlideReach>();
	/** Why there is no answer, in words. */
	problem = $state.raw<string>();
	/** Whether the core is being asked. */
	asking = $state(false);

	#ask: AskReach;
	#forget: () => void;
	#storage: Storage;
	#asked = 0;

	constructor(storage: Storage, ask: AskReach, forget: () => void = () => {}) {
		this.#storage = storage;
		this.#ask = ask;
		this.#forget = forget;
	}

	/** "What a glide reaches from here": the area from a place, with the pilot's glide. */
	open(place: Coordinate): Promise<void> {
		this.place = place;
		this.reach = undefined;
		this.problem = undefined;
		this.glide = keptSetting(this.#storage).glide;
		this.startHeightM = undefined;
		return this.show();
	}

	/** "Clear": no area, and the ground fetched for it let go of. */
	clear(): void {
		this.#asked += 1;
		this.place = undefined;
		this.reach = undefined;
		this.problem = undefined;
		this.asking = false;
		this.#forget();
	}

	/** What the core is asked, for the place and the settings as they stand. */
	setting(place: Coordinate): GlideReachSetting {
		return {
			lat: place.lat,
			lon: place.lon,
			glideRatio: this.glide,
			startHeightM: this.startHeightM,
			windFromDeg: this.windOn ? this.windFromDeg : undefined,
			windKmh: this.windOn ? this.windKmh : undefined,
			marginM: undefined
		};
	}

	/** Asks the core again, for a setting changed. */
	async show(): Promise<void> {
		const place = this.place;
		if (!place) return;
		const mine = ++this.#asked;
		this.asking = true;
		try {
			const reach = await this.#ask(this.setting(place));
			if (mine !== this.#asked) return;
			this.reach = reach;
			this.problem = undefined;
		} catch (error) {
			if (mine !== this.#asked) return;
			this.reach = undefined;
			this.problem = messageOf(error);
		} finally {
			if (mine === this.#asked) this.asking = false;
		}
	}
}
