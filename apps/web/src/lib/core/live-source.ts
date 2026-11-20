/**
 * Who is in the air, from the Venti server, as the worker fetches it: the
 * gliders whose pilots show their flight to everyone, and the flight of one
 * of them.
 *
 * The core says what to ask the server for and reads its answers. This file
 * sends the requests in between, and it is where the web decides how an
 * answer is kept, which matters here as nowhere else.
 *
 * The server lets these answers be kept and checked again ("no-cache" with
 * a validator), and while nothing has changed it answers "not modified"
 * with no body. A browser's own cache would then hand the page the body it
 * kept, with the server's clock of an earlier moment in it, and the age of
 * every position would start again from that moment at each question. So
 * the requests go past the browser's cache, and this file checks by hand:
 * it names the validator of its last answer to the same address, and when
 * the server says "not modified" it hands out that last answer again,
 * marked as the same answer. Ages count on from the last answer that the
 * server gave in full, as the API's document asks.
 *
 * The server is the one this site is served from, as for the wind.
 */

import {
	type Bounds,
	liveAgeInWords,
	LiveFollow,
	type LiveFollowView,
	liveMap,
	type LiveMap,
	liveMapPath
} from './pkg/venti_core_wasm.js';
import { unreachable } from './protocol';

/** What a request brought back, with the validator the server gave the answer. */
export interface LiveFetched {
	status: number;
	body: Uint8Array;
	etag: string | undefined;
}

/**
 * Fetches a path from the server, past every cache. `etag` is the validator
 * of the answer held for the same path, if one is held: the server then
 * answers 304 with no body while nothing has changed. Throws when no answer
 * came at all.
 */
export type LiveGet = (path: string, etag: string | undefined) => Promise<LiveFetched>;

/** How long a request may take before the server counts as out of reach, in milliseconds. */
const PATIENCE_MS = 10_000;

/** Fetches from the origin this site is served from. */
export const getLiveFromThisOrigin: LiveGet = async (path, etag) => {
	try {
		const response = await fetch(path, {
			// A position that is kept is a wrong one. With a validator named by
			// hand the browser also hands the 304 over as it is.
			cache: 'no-store',
			headers: { accept: 'application/json', ...(etag ? { 'if-none-match': etag } : {}) },
			signal: AbortSignal.timeout(PATIENCE_MS)
		});
		return {
			status: response.status,
			body: new Uint8Array(await response.arrayBuffer()),
			etag: response.headers.get('etag') ?? undefined
		};
	} catch {
		throw unreachable('Venti could not be reached for who is in the air.');
	}
};

/** Who is in the air in a box of the map. */
export interface LiveAnswer {
	map: LiveMap;
	/**
	 * Which of the server's answers this is: a number that goes up with
	 * every answer the server gives in full. The same number as last time
	 * says that the server had nothing new, and that the ages in `map` are
	 * still those of the moment that answer first came.
	 */
	heard: number;
}

/** One flight being followed, as far as the server has handed it out. */
export type LiveFollowAnswer =
	| {
			view: LiveFollowView;
			/** As in {@link LiveAnswer}. */
			heard: number;
	  }
	/** The track is gone from the server. Why, in the core's words. */
	| { gone: string };

/** An answer held for the address it was asked from. */
interface Held<Answer> {
	path: string;
	etag: string | undefined;
	answer: Answer;
}

export class LiveSource {
	/** Whether this site has a server at all. One without is never asked. */
	#present: () => Promise<boolean>;
	#get: LiveGet;
	/** Counts the answers the server gave in full. */
	#heard = 0;
	/** The last answer about a part of the map. */
	#map: Held<LiveAnswer> | undefined;
	/** The flight being followed: the core's reader of it, and its last answer. */
	#followed:
		| {
				id: string;
				follow: LiveFollow;
				last: Held<{ view: LiveFollowView; heard: number }> | undefined;
				/** How many requests for it are on their way. */
				asking: number;
		  }
		| undefined;

	constructor(
		present: () => Promise<boolean> = async () => true,
		get: LiveGet = getLiveFromThisOrigin
	) {
		this.#present = present;
		this.#get = get;
	}

	/**
	 * The gliders in a box of the map. Nothing if this site has no server.
	 * Throws, with words for the pilot, when the server gives no usable
	 * answer.
	 */
	async gliders(box: Bounds): Promise<LiveAnswer | undefined> {
		if (!(await this.#present())) return undefined;
		const path = liveMapPath(box.south, box.west, box.north, box.east);
		const held = this.#map?.path === path ? this.#map : undefined;
		const fetched = await this.#get(path, held?.etag);
		// Nothing has changed: the answer held is still the server's.
		if (fetched.status === 304 && held) return held.answer;
		const answer = { map: liveMap(fetched.status, fetched.body), heard: ++this.#heard };
		this.#map = { path, etag: fetched.etag, answer };
		return answer;
	}

	/**
	 * Asks the server once about the flight of that identifier: for the
	 * whole track the first time, afterwards for what has arrived since.
	 * The page asks again every ten seconds, and at once while the answer's
	 * `more` is true.
	 *
	 * Nothing if this site has no server, and nothing when the page has
	 * turned to another flight before the server answered. Throws, with
	 * words for the pilot, when the server gives no usable answer; a track
	 * that is gone is not thrown but answered.
	 */
	async follow(id: string): Promise<LiveFollowAnswer | undefined> {
		if (!(await this.#present())) return undefined;
		let followed = this.#followed;
		if (followed?.id !== id) {
			this.unfollow();
			followed = this.#followed = { id, follow: new LiveFollow(id), last: undefined, asking: 0 };
		}
		const { follow } = followed;
		const path = follow.path();
		const held = followed.last?.path === path ? followed.last : undefined;

		let fetched: LiveFetched;
		followed.asking += 1;
		try {
			fetched = await this.#get(path, held?.etag);
		} finally {
			followed.asking -= 1;
			// The page let go of this flight meanwhile: its reader is free to go.
			if (this.#followed !== followed && followed.asking === 0) follow.free();
		}
		if (this.#followed !== followed) return undefined;
		if (fetched.status === 304 && held) return held.answer;

		try {
			const answer = { view: follow.receive(fetched.status, fetched.body), heard: ++this.#heard };
			followed.last = { path, etag: fetched.etag, answer };
			return answer;
		} catch (error) {
			if (!follow.isOver()) throw error;
			return { gone: error instanceof Error ? error.message : String(error) };
		}
	}

	/** Lets go of the flight being followed, and of every position held for it. */
	unfollow(): void {
		const followed = this.#followed;
		this.#followed = undefined;
		// A request still on its way lets go of the reader when it comes back.
		if (followed && followed.asking === 0) followed.follow.free();
	}

	/**
	 * How old positions are, in the core's words: "Now", "40 s ago",
	 * "Landed 12 min ago", "No position for 6 min". One answer for each
	 * question, in their order.
	 */
	ages(asked: readonly { ageS: number; landed: boolean }[]): string[] {
		return asked.map(({ ageS, landed }) => liveAgeInWords(ageS, landed));
	}
}
