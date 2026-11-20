/**
 * Who is in the air, as the screens see it: the gliders whose pilots show
 * their flight to everyone, around the part of the map in view, and the
 * flight of the one whose page is open.
 *
 * `watch.ts` holds the rules. This class gives them the page: the core in
 * its worker to ask, timers, and whether the tab is in front. A tab that
 * nobody looks at asks the server nothing.
 *
 * The screens call `show`, `follow` and `tellAges` from effects. None of
 * the three lets what it reads on the way become something its caller
 * waits for: an effect that began to follow a flight would otherwise begin
 * again with every answer about it.
 */

import { untrack } from 'svelte';
import type { Bounds, CoreClient, LiveGlider } from '#lib/core/index.ts';
import {
	ageNow,
	ASK_EVERY_MS,
	FlightFollow,
	type Followed,
	type LiveState,
	LiveWatch,
	NO_LIVE,
	NOT_FOLLOWED
} from './watch';

/** How long the map has to rest before the server is asked, in milliseconds. */
const SETTLE_MS = 400;

/** How often the ages in words are brought up to date, in milliseconds. */
const AGES_EVERY_MS = 1000;

const inFront = () => document.visibilityState === 'visible';

export class LiveModel {
	/** The gliders as last learned. */
	state = $state.raw<LiveState>(NO_LIVE);

	/** The flight being followed: the glider whose page is open, with its track so far. */
	followed = $state.raw<Followed>(NOT_FOLLOWED);

	/**
	 * How old each glider's position is now, in the core's words, by the
	 * track's identifier. Kept current while {@link LiveModel.tellAges} runs.
	 */
	ages = $state.raw<ReadonlyMap<string, string>>(new Map());

	/** The same for the glider of the flight being followed. */
	followedAge = $state.raw('');

	#core: CoreClient;
	#watch: LiveWatch;
	#settling: ReturnType<typeof setTimeout> | undefined;
	/** How many screens are showing ages right now, and the timer that serves them. */
	#readers = 0;
	#telling: ReturnType<typeof setInterval> | undefined;

	constructor(core: CoreClient) {
		this.#core = core;
		this.#watch = new LiveWatch(
			(box) => core.live(box),
			(state) => {
				this.state = state;
				this.#tellAgesNow();
			}
		);
	}

	/** The map says which part of it is in view. */
	show(region: Bounds): void {
		const ask = untrack(() => this.#watch.show(region));
		clearTimeout(this.#settling);
		// The map reports every pause of a drag; wait until it rests.
		if (ask) this.#settling = setTimeout(() => void this.#watch.load(), SETTLE_MS);
	}

	/**
	 * Asks again every quarter of a minute for as long as the map is on
	 * screen and the tab is in front. The first question is asked when the
	 * map says what it shows. Returns what to call when the map leaves the
	 * screen.
	 */
	keepCurrent(): () => void {
		const timer = setInterval(() => {
			if (inFront()) void this.#watch.load();
		}, ASK_EVERY_MS);
		// A tab that comes back to the front catches up at once.
		const back = () => {
			if (inFront()) void this.#watch.load();
		};
		document.addEventListener('visibilitychange', back);
		return () => {
			clearInterval(timer);
			clearTimeout(this.#settling);
			document.removeEventListener('visibilitychange', back);
		};
	}

	/**
	 * Follows one glider's flight: the whole track first, then what arrives,
	 * every ten seconds while the tab is in front. Returns what to call when
	 * the glider's page closes.
	 */
	follow(id: string): () => void {
		return untrack(() => this.#follow(id));
	}

	#follow(id: string): () => void {
		let stopped = false;
		let waiting: ReturnType<typeof setTimeout> | undefined;
		/** Whether a step fell due while the tab was not in front. */
		let due = false;

		const flight = new FlightFollow(
			id,
			(id) => this.#core.followLive(id),
			(followed) => {
				if (stopped) return;
				this.followed = followed;
				this.#tellAgesNow();
			}
		);
		const step = async () => {
			if (stopped) return;
			if (!inFront()) {
				due = true;
				return;
			}
			const wait = await flight.step();
			if (stopped || wait === undefined) return;
			waiting = setTimeout(() => void step(), wait);
		};
		const back = () => {
			if (!due || !inFront()) return;
			due = false;
			void step();
		};
		document.addEventListener('visibilitychange', back);
		void step();

		return () => {
			stopped = true;
			clearTimeout(waiting);
			document.removeEventListener('visibilitychange', back);
			if (this.followed.id === id) {
				this.followed = NOT_FOLLOWED;
				this.followedAge = '';
			}
			// The worker holds every position of the flight until told.
			void this.#core.unfollowLive().catch(() => {});
		};
	}

	/**
	 * A glider as the newest answer about it has it: the one that follows
	 * its flight, if one does, or else the map's.
	 */
	glider(id: string): LiveGlider | undefined {
		const followed = this.followed.view?.glider;
		return followed?.id === id ? followed : this.state.gliders.find((glider) => glider.id === id);
	}

	/** How old a glider's position is now, in the core's words: "Now", "45 s ago". */
	ageInWords(glider: LiveGlider): string {
		return glider === this.followed.view?.glider
			? this.followedAge
			: (this.ages.get(glider.id) ?? '');
	}

	/**
	 * Keeps the ages in words current, second by second, for as long as a
	 * screen shows them. Returns what to call when it no longer does.
	 */
	tellAges(): () => void {
		this.#readers += 1;
		this.#telling ??= setInterval(() => this.#tellAgesNow(), AGES_EVERY_MS);
		untrack(() => this.#tellAgesNow());
		return () => {
			this.#readers -= 1;
			if (this.#readers > 0) return;
			clearInterval(this.#telling);
			this.#telling = undefined;
		};
	}

	/**
	 * Works out every age by this device's clock and has the core put it in
	 * words: which words an age gets is the core's to say.
	 */
	#tellAgesNow(): void {
		if (this.#readers === 0) return;
		const now = Date.now();
		const { gliders, answeredAt } = this.state;
		const followed = this.followed.view?.glider;
		const asked = gliders.map((glider) => ({
			ageS: ageNow(glider, answeredAt, now),
			landed: glider.landed
		}));
		if (followed) {
			asked.push({
				ageS: ageNow(followed, this.followed.answeredAt, now),
				landed: followed.landed
			});
		}
		if (asked.length === 0) return;
		void this.#core.liveAges(asked).then(
			(words) => {
				// What was asked about may have been replaced meanwhile.
				if (gliders === this.state.gliders) {
					const ages = new Map(gliders.map((glider, index) => [glider.id, words[index]]));
					// Most seconds change no word, and then no screen has to be drawn again.
					const before = this.ages;
					if (ages.size !== before.size || [...ages].some(([id, age]) => before.get(id) !== age)) {
						this.ages = ages;
					}
				}
				if (followed && followed === this.followed.view?.glider) {
					this.followedAge = words[gliders.length];
				}
			},
			() => {}
		);
	}
}
