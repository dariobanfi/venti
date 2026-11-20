/**
 * Keeps the gliders for the part of the map in view, and one flight that
 * is being followed: when to ask the server, what to keep while it does not
 * answer, and by which moment a position's age is counted. The iPhone app's
 * `LiveModel.swift` follows the same rules.
 *
 * What to ask and how to read the answer is the core's, in the worker. This
 * file knows nothing about the page: `model.svelte.ts` connects it to the
 * screens, to timers and to whether the tab is in front.
 */

import type {
	Bounds,
	LiveAnswer,
	LiveFollowAnswer,
	LiveFollowView,
	LiveGlider
} from '#lib/core/index.ts';
import { contains, grown } from '#lib/wind/watch.ts';

/**
 * How often the server is asked again who is in the air, in milliseconds.
 * It asks for ten to fifteen seconds.
 */
export const ASK_EVERY_MS = 15_000;

/**
 * How often a followed flight is asked for again, in milliseconds. The
 * server asks for five to ten seconds.
 */
export const FOLLOW_EVERY_MS = 10_000;

/**
 * A map that shows more than this many degrees of longitude asks for no
 * gliders: the Alps from end to end are a crowd, not pilots.
 */
export const WIDEST_MAP_DEG = 6;

/** How much more than the part in view is asked for, so that a small drag needs no new request. */
const GROWN_BY = 0.25;

/** What the screens are shown. */
export interface LiveState {
	/** Whether the map shows little enough of the land for gliders to be asked for. */
	close: boolean;
	/** Whether the server has answered for the part of the map in view. */
	known: boolean;
	/** The gliders around the part of the map in view. */
	gliders: LiveGlider[];
	/**
	 * When the answer came that the gliders' ages are from, by this device's
	 * clock, in milliseconds. A position's age runs on from there.
	 */
	answeredAt: number;
	/** Whether the area holds more gliders than the server lists at once. */
	truncated: boolean;
}

export const NO_LIVE: LiveState = {
	close: false,
	known: false,
	gliders: [],
	answeredAt: 0,
	truncated: false
};

export class LiveWatch {
	#ask: (box: Bounds) => Promise<LiveAnswer | undefined>;
	#tell: (state: LiveState) => void;
	#now: () => number;
	#state = NO_LIVE;
	/** The part of the map in view, while it is not too wide. */
	#wanted: Bounds | undefined;
	/** The area last asked for. */
	#asked: Bounds | undefined;
	/** The area being asked for right now. */
	#asking: Bounds | undefined;
	/** The last answer the server gave in full: its number, what it held, and when it came. */
	#answered: { heard: number; gliders: LiveGlider[]; truncated: boolean; at: number } | undefined;
	/** Whether the site turned out to have no server. */
	#absent = false;
	/** Counts the requests, so that an answer overtaken by a later one is dropped. */
	#requests = 0;

	constructor(
		ask: (box: Bounds) => Promise<LiveAnswer | undefined>,
		tell: (state: LiveState) => void,
		now: () => number = Date.now
	) {
		this.#ask = ask;
		this.#tell = tell;
		this.#now = now;
	}

	/**
	 * The map says which part of it is in view. Returns whether the server
	 * has to be asked now: not when the map shows too much, and not when
	 * that part lies inside what was asked for, which the next round brings
	 * up to date.
	 */
	show(region: Bounds): boolean {
		if (region.east - region.west > WIDEST_MAP_DEG) {
			this.#forget();
			return false;
		}
		this.#wanted = region;
		this.#show();
		return !this.#absent && !(this.#asked && contains(this.#asked, region));
	}

	/**
	 * Asks the server who is in the air around the part of the map in view:
	 * the area asked for before while the map is still inside it, a little
	 * more than is in view otherwise. Called when the map has come to rest
	 * somewhere new, and again every quarter of a minute.
	 *
	 * An answer that does not come leaves the gliders where they were: their
	 * ages go on counting, which is what is known about them.
	 */
	async load(): Promise<void> {
		const region = this.#wanted;
		if (!region || this.#absent) return;
		const area =
			this.#asked && contains(this.#asked, region) ? this.#asked : grown(region, GROWN_BY);
		// The same question is on its way already.
		if (this.#asking && same(this.#asking, area)) return;

		const request = ++this.#requests;
		this.#asking = area;
		let answer: LiveAnswer | undefined;
		try {
			answer = await this.#ask(area);
		} catch {
			// A server from before live tracks, or none in reach: nothing new
			// is known. The next round asks for the same again.
			if (request === this.#requests) this.#asked ??= area;
			return;
		} finally {
			if (request === this.#requests) this.#asking = undefined;
		}
		// The map has moved on, or shows too much by now.
		if (request !== this.#requests || !this.#wanted) return;
		if (!answer) {
			// A site without a server has a map without gliders, and says
			// nothing about it.
			this.#absent = true;
			this.#forget();
			return;
		}
		this.#asked = area;
		// "Nothing new" from the server renews nothing: the ages are still
		// those of the answer it gave in full.
		if (answer.heard !== this.#answered?.heard) {
			this.#answered = {
				heard: answer.heard,
				gliders: answer.map.gliders,
				truncated: answer.map.truncated,
				at: this.#now()
			};
		}
		this.#show();
	}

	/** The map shows too much, or there is nobody to ask: no gliders, and no question waiting. */
	#forget(): void {
		this.#wanted = undefined;
		this.#asked = undefined;
		this.#asking = undefined;
		this.#answered = undefined;
		// Whatever is on its way is not wanted any more.
		this.#requests += 1;
		this.#show();
	}

	#show(): void {
		const answered = this.#answered;
		const state: LiveState = {
			close: this.#wanted !== undefined,
			known: Boolean(
				answered && this.#wanted && this.#asked && contains(this.#asked, this.#wanted)
			),
			gliders: answered?.gliders ?? NO_LIVE.gliders,
			answeredAt: answered?.at ?? 0,
			truncated: answered?.truncated ?? false
		};
		const before = this.#state;
		if (
			state.close === before.close &&
			state.known === before.known &&
			state.gliders === before.gliders &&
			state.answeredAt === before.answeredAt &&
			state.truncated === before.truncated
		) {
			return;
		}
		this.#state = state;
		this.#tell(state);
	}
}

function same(first: Bounds, second: Bounds): boolean {
	return (
		first.south === second.south &&
		first.west === second.west &&
		first.north === second.north &&
		first.east === second.east
	);
}

/** One flight being followed, as the screens are shown it. */
export interface Followed {
	/** The identifier of the track, or nothing while no flight is followed. */
	id: string | undefined;
	/** The flight so far, once the server has handed its track out. */
	view: LiveFollowView | undefined;
	/** When the answer came that the glider's age is from, in milliseconds. */
	answeredAt: number;
	/** Why the flight cannot be followed, in words for the pilot. */
	problem: string | undefined;
}

export const NOT_FOLLOWED: Followed = {
	id: undefined,
	view: undefined,
	answeredAt: 0,
	problem: undefined
};

/**
 * Follows one flight: the whole track first, then what arrives. Whoever
 * holds it calls {@link FlightFollow.step} again after the time each step
 * names.
 */
export class FlightFollow {
	#id: string;
	#ask: (id: string) => Promise<LiveFollowAnswer | undefined>;
	#tell: (followed: Followed) => void;
	#now: () => number;
	/** The number of the last answer the server gave in full. */
	#heard: number | undefined;

	constructor(
		id: string,
		ask: (id: string) => Promise<LiveFollowAnswer | undefined>,
		tell: (followed: Followed) => void,
		now: () => number = Date.now
	) {
		this.#id = id;
		this.#ask = ask;
		this.#tell = tell;
		this.#now = now;
		tell({ ...NOT_FOLLOWED, id });
	}

	/**
	 * Asks the server once. Returns how long to wait before the next step,
	 * in milliseconds: no time at all while the server has more of the track
	 * waiting. Returns nothing when there is no next step: the track is gone
	 * from the server, which is then said in words, or there is nobody to
	 * ask.
	 */
	async step(): Promise<number | undefined> {
		let answer: LiveFollowAnswer | undefined;
		try {
			answer = await this.#ask(this.#id);
		} catch {
			// The server could not be asked, or was in trouble: the track
			// stays as it was, with its age counting on.
			return FOLLOW_EVERY_MS;
		}
		if (!answer) return undefined;
		if ('gone' in answer) {
			this.#tell({ id: this.#id, view: undefined, answeredAt: 0, problem: answer.gone });
			return undefined;
		}
		if (answer.heard !== this.#heard) {
			this.#heard = answer.heard;
			this.#tell({ id: this.#id, view: answer.view, answeredAt: this.#now(), problem: undefined });
		}
		return answer.view.more ? 0 : FOLLOW_EVERY_MS;
	}
}

/**
 * How old a position is now, in seconds: its age when the server answered,
 * and the time since by this device's clock.
 */
export function ageNow(glider: LiveGlider, answeredAt: number, now: number): number {
	return glider.ageS + Math.max(now - answeredAt, 0) / 1000;
}
