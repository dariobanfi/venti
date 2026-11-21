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
	type Credit,
	liveAgeInWords,
	LiveFollow,
	type LiveFollowView,
	liveLayer,
	liveMap,
	type LiveMap,
	liveMapPath,
	liveOthers,
	type LiveOthersNetwork,
	liveOthersPath,
	othersTooWideWords
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
	/**
	 * What other networks hear in the same box (the Open Glider Network),
	 * beside Venti's own pilots and never one record with them. Nothing
	 * where the server lists no others (one from before them, or one that
	 * did not answer), and where the box is too wide for them.
	 */
	others: LiveOthersAnswer | undefined;
	/** Whether the box was too wide to ask for others: the map then says {@link LiveAnswer.tooWideWords}. */
	othersTooWide: boolean;
	/** What a map says where it shows too much to ask for others, in the core's words. */
	tooWideWords: string;
}

/** The aircraft other networks hear in a box, as the core puts them beside Venti's pilots. */
export interface LiveOthersAnswer {
	/**
	 * The networks the server lists, each with its aircraft (paragliders
	 * and hang gliders first) and its credit. Drawn whether or not the
	 * server is connected to them now: it lists only what it heard in the
	 * last ten minutes, and each position says its age.
	 */
	networks: LiveOthersNetwork[];
	/** Who is in the air, in the core's words: "3 pilots of Venti, 5 others in the air". */
	countWords: string;
	/** The core's credits to show with the map: those of the networks with an aircraft in it. */
	credits: Credit[];
	/** As {@link LiveAnswer.heard}: the same number while neither answer has changed. */
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
	/** The last answer about a part of the map, with the body the server gave in full. */
	#map: Held<{ map: LiveMap; heard: number; body: Uint8Array }> | undefined;
	/** The last answer about the others in a part of the map, as the server gave it. */
	#others: Held<{ heard: number; body: Uint8Array }> | undefined;
	/** The others as last put beside Venti's pilots, by the two answers they are from. */
	#layer: { from: string; others: LiveOthersAnswer } | undefined;
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
		const othersPath = liveOthersPath(box.south, box.west, box.north, box.east);
		// The two questions go together; others that do not come leave
		// Venti's own pilots as they are.
		const [venti, others] = await Promise.all([
			this.#venti(liveMapPath(box.south, box.west, box.north, box.east)),
			othersPath === undefined ? undefined : this.#othersAt(othersPath).catch(() => undefined)
		]);
		return {
			map: venti.map,
			heard: venti.heard,
			others: others && this.#beside(venti, others),
			othersTooWide: othersPath === undefined,
			tooWideWords: othersTooWideWords()
		};
	}

	/** Venti's own pilots in a box: the answer held while the server has nothing new. */
	async #venti(path: string) {
		const held = this.#map?.path === path ? this.#map : undefined;
		const fetched = await this.#get(path, held?.etag);
		if (fetched.status === 304 && held) return held.answer;
		const answer = {
			map: liveMap(fetched.status, fetched.body),
			heard: ++this.#heard,
			body: fetched.body
		};
		this.#map = { path, etag: fetched.etag, answer };
		return answer;
	}

	/**
	 * The others in a box, as the server gave them in full. Throws where
	 * the server has no such list, or gives no usable answer.
	 */
	async #othersAt(path: string) {
		const held = this.#others?.path === path ? this.#others : undefined;
		const fetched = await this.#get(path, held?.etag);
		if (fetched.status === 304 && held) return held.answer;
		// Read once to know the answer is one; the core reads it again beside Venti's.
		liveOthers(fetched.status, fetched.body);
		const answer = { heard: ++this.#heard, body: fetched.body };
		this.#others = { path, etag: fetched.etag, answer };
		return answer;
	}

	/** The others beside Venti's pilots, as the core puts them: worked out again only when either answer is new. */
	#beside(venti: { heard: number; body: Uint8Array }, others: { heard: number; body: Uint8Array }) {
		const from = `${venti.heard}:${others.heard}`;
		if (this.#layer?.from === from) return this.#layer.others;
		const layer = liveLayer(200, venti.body, 200, others.body);
		const answer: LiveOthersAnswer = {
			networks: layer.others,
			countWords: layer.countWords,
			credits: layer.credits,
			heard: ++this.#heard
		};
		this.#layer = { from, others: answer };
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
