/**
 * Keeps the wind for the part of the map in view: when to ask the server,
 * what to keep while it does not answer, and what to tell the pilot. The
 * iPhone app's `WindModel.swift` follows the same rules.
 *
 * What to ask and how to read the answer is the core's, in the worker. This
 * file knows nothing about the page: `model.svelte.ts` connects it to the
 * screens and to the clock on the wall.
 */

import type { Bounds, Credit, WindMap, WindStation } from '#lib/core/index.ts';
import { isInWords } from '#lib/core/index.ts';

/** What the screens are shown. */
export interface WindState {
	/**
	 * Whether this site has a Venti server to ask. Not known before the first
	 * answer; without one the map has no wind and says nothing about it.
	 */
	available: boolean | undefined;
	/** The stations around the part of the map in view. */
	stations: WindStation[];
	/** Whose data they are. The licences ask for this next to the data. */
	credits: Credit[];
	/** Whether the area holds more stations than the server lists at once. */
	truncated: boolean;
	/** Why the wind is missing or not current, in words for the pilot. */
	problem: string | undefined;
	/**
	 * The core's sentence about the networks with stations here that sent
	 * nothing new for a while; none when every one delivers.
	 */
	lateNotice: string | undefined;
}

export const NO_WIND: WindState = {
	available: undefined,
	stations: [],
	credits: [],
	truncated: false,
	problem: undefined,
	lateNotice: undefined
};

/**
 * How often the wind is asked for again, in milliseconds. The server lets
 * an answer be reused for this long, and stations report every ten minutes.
 */
export const REFRESH_MS = 60_000;

/**
 * After this long without an answer the stations are taken off the map: an
 * arrow must not outlive the wind it stood for.
 */
export const KEPT_WITHOUT_ANSWER_MS = 15 * 60_000;

/** How much more than the part in view is asked for, so a small drag needs no request. */
const GROWN_BY = 0.25;

/** Whether another box lies wholly inside this one. */
export function contains(box: Bounds, other: Bounds): boolean {
	return (
		box.south <= other.south &&
		box.north >= other.north &&
		box.west <= other.west &&
		box.east >= other.east
	);
}

/** The box widened on every side by a share of its own size. */
export function grown(box: Bounds, share: number): Bounds {
	const height = (box.north - box.south) * share;
	const width = (box.east - box.west) * share;
	return {
		south: box.south - height,
		west: box.west - width,
		north: box.north + height,
		east: box.east + width
	};
}

export class WindWatch {
	#ask: (box: Bounds) => Promise<WindMap | undefined>;
	#tell: (state: WindState) => void;
	#now: () => number;
	#state = NO_WIND;
	/** The part of the map in view. */
	#wanted: Bounds | undefined;
	/** The area the stations were asked for, and when the answer came. */
	#answered: { area: Bounds; at: number } | undefined;
	/** Counts the requests, so that an answer overtaken by a later one is dropped. */
	#asked = 0;

	constructor(
		ask: (box: Bounds) => Promise<WindMap | undefined>,
		tell: (state: WindState) => void,
		now: () => number = Date.now
	) {
		this.#ask = ask;
		this.#tell = tell;
		this.#now = now;
	}

	/**
	 * The map says which part of it is in view. Returns whether the server
	 * has to be asked: not when that part is covered by a recent answer.
	 */
	show(region: Bounds): boolean {
		this.#wanted = region;
		if (this.#state.available === false) return false;
		const answered = this.#answered;
		return !(
			answered &&
			!this.#state.truncated &&
			contains(answered.area, region) &&
			this.#now() - answered.at < REFRESH_MS
		);
	}

	/** Whether the last answer is old enough to ask again. */
	get due(): boolean {
		if (this.#state.available === false || !this.#wanted) return false;
		return !this.#answered || this.#now() - this.#answered.at >= REFRESH_MS;
	}

	/** Asks the server for the stations around the part of the map in view. */
	async load(): Promise<void> {
		const region = this.#wanted;
		if (!region || this.#state.available === false) return;
		const area = grown(region, GROWN_BY);
		const asked = ++this.#asked;
		try {
			const answer = await this.#ask(area);
			if (asked !== this.#asked) return;
			if (!answer) {
				this.#set({ ...NO_WIND, available: false });
				return;
			}
			this.#answered = { area, at: this.#now() };
			this.#set({
				available: true,
				stations: answer.stations,
				credits: answer.credits,
				truncated: answer.truncated,
				problem: undefined,
				lateNotice: answer.lateNotice
			});
		} catch (error) {
			if (asked !== this.#asked) return;
			const lost = !this.#answered || this.#now() - this.#answered.at > KEPT_WITHOUT_ANSWER_MS;
			this.#set({
				...this.#state,
				// Something answered, even if not usefully: there is a server.
				available: this.#state.available ?? true,
				problem: words(error),
				...(lost ? { stations: [], credits: [], truncated: false } : {})
			});
		}
	}

	#set(state: WindState): void {
		this.#state = state;
		this.#tell(state);
	}
}

/** Why the wind could not be fetched, for the pilot. */
function words(error: unknown): string {
	const said = error instanceof Error ? error.message : String(error);
	return isInWords(error) ? said : `The wind could not be shown. ${said}`;
}

/** What the pilot should know about the wind on the map, if anything. */
export function notice(state: WindState): string {
	if (state.problem) return state.problem;
	if (state.truncated) {
		return 'There are more stations here than fit on the map. Zoom in to see them all.';
	}
	return state.lateNotice ?? '';
}
