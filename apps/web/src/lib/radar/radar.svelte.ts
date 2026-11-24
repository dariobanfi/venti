/**
 * The rain radar as the map tab sees it: asks the core in its worker for
 * the list of frames and for each frame's picture, keeps the pictures of
 * this visit, and remembers which moment the pilot looks at.
 *
 * A moment draws a picture for each layer that has one then, and they
 * change together. What is on the map is always a moment whole, with its
 * own time: while another moment's pictures are asked for, or one of them
 * cannot be had, those before stay, and the card names that moment.
 * `radar.ts` holds the rules.
 */

import type { CoreClient, RadarMoment, RadarView } from '#lib/core/index.ts';
import { inWordsOr } from '#lib/core/index.ts';
import {
	AGES_EVERY_MS,
	cautioned,
	chosenTime,
	colours,
	type Corners,
	type Drawn,
	drawn,
	drawnKey,
	isDue,
	momentOf,
	pictureAddress,
	type Pictured,
	pictureKey,
	PLAY_MS,
	WORDS
} from './radar';

type Core = Pick<CoreClient, 'radar' | 'radarAgain' | 'radarPicture'>;

/** One layer's picture on the map: where the map loads it from, and how it is drawn. */
export interface OnMap {
	/** Which layer: its place in the view's layers. The map draws a higher one over a lower. */
	layer: number;
	/** The address the map loads the picture from. */
	url: string;
	corners: Corners;
	/** Whether the layer is too old to be the rain of now: drawn at half its strength. */
	faded: boolean;
}

const inFront = () => (globalThis.document?.visibilityState ?? 'visible') === 'visible';

const pause = (ms: number) => new Promise<void>((done) => setTimeout(done, ms));

export class RadarModel {
	/** The newest list read in this visit. */
	view = $state.raw<RadarView>();
	/** Whether the server is being asked for the list. */
	asking = $state(false);
	/** Why the last asking brought no list, in words for the pilot. */
	problem = $state<string>();
	/** The time of the moment the pilot chose; nothing for the one the core opens on, the newest. */
	chosenTimeUnixS = $state<number>();
	/**
	 * The pictures on the map, the lowest first, with the moment they are
	 * of as it was when they were fetched.
	 */
	shown = $state.raw<{ moment: RadarMoment; pictures: (Drawn & { url: string })[] }>();
	/** Whether the chosen moment's pictures are being asked for. */
	pictureAsking = $state(false);
	/** Why the chosen moment's pictures could not be had, in words. */
	pictureProblem = $state<string>();
	/** Whether the moments are being played. */
	playing = $state(false);

	/** The stops of the card's strip, the oldest first: one line of time for all layers. */
	readonly moments = $derived(this.view?.moments ?? []);
	/** The moment the strip marks: the pilot's, or the newest. */
	readonly moment = $derived(momentOf(this.view, this.chosenTimeUnixS));
	/**
	 * The moment whose pictures are on the map, as the newest list words
	 * it, so that its age is that of now. Where the list has other pictures
	 * for that moment than the map, it is the moment as it was when the
	 * map's were fetched.
	 */
	readonly shownMoment = $derived.by(() => {
		const shown = this.shown;
		const view = this.view;
		if (!shown || !view) return shown?.moment;
		const key = drawnKey(shown.pictures);
		return (
			view.moments.find(
				(moment) =>
					moment.timeUnixS === shown.moment.timeUnixS && drawnKey(drawn(view, moment)) === key
			) ?? shown.moment
		);
	});
	/**
	 * What the map draws, the lowest first: each picture as a layer of its
	 * own, with its corners, and whether its layer is too old to be the rain
	 * of now, each by its own.
	 */
	readonly onMap = $derived.by((): OnMap[] => {
		const layers = this.view?.layers ?? [];
		return (this.shown?.pictures ?? []).map((picture) => ({
			layer: picture.layer,
			url: picture.url,
			corners: picture.corners,
			faded: layers[picture.layer]?.stale ?? picture.faded
		}));
	});
	/** The layers that have a picture on the map, the lowest first: whose credits are shown. */
	readonly layersDrawn = $derived(
		this.onMap.flatMap((picture) => this.view?.layers[picture.layer] ?? [])
	);
	/**
	 * The core's sentence when the rain shown is too old to be the rain of
	 * now: that of the topmost layer that has frames. Nothing while it is not.
	 */
	readonly caution = $derived.by(() => {
		const layer = cautioned(this.view);
		return layer?.stale ? layer.staleWords : undefined;
	});

	#core: Core;
	#clock: () => number;
	#wait: (ms: number) => Promise<void>;
	#askedMs: number | undefined;
	/**
	 * The pictures fetched in this visit as addresses the map loads: each
	 * layer's frames by their paths, by whether the veil is drawn in them,
	 * and by the frames that lie over them (`pictureKey`).
	 */
	#pictures = new Map<string, Promise<string>>();
	/** Counts the moments asked for and the plays, so that a slow answer is dropped. */
	#momentsAsked = 0;
	#plays = 0;

	constructor(
		core: Core,
		clock: () => number = Date.now,
		wait: (ms: number) => Promise<void> = pause
	) {
		this.#core = core;
		this.#clock = clock;
		this.#wait = wait;
	}

	/** Shows the moment of a cell of the strip; choosing by hand ends a play. */
	choose(index: number): void {
		this.stop();
		if (this.view) this.chosenTimeUnixS = chosenTime(this.view, index);
	}

	/**
	 * Asks for the pictures of the moment the strip marks, unless they are
	 * at hand, and puts them on the map together. Call it again whenever
	 * that moment changes.
	 */
	async showMoment(): Promise<void> {
		const moment = this.moment;
		const view = this.view;
		const mine = ++this.#momentsAsked;
		if (!moment || !view) {
			// No layer has a frame: nothing of an earlier list stays on the map.
			if (view) this.shown = undefined;
			this.pictureAsking = false;
			this.pictureProblem = undefined;
			return;
		}
		const wanted = drawn(view, moment);
		if (this.shown && drawnKey(this.shown.pictures) === drawnKey(wanted)) {
			this.pictureAsking = false;
			this.pictureProblem = undefined;
			return;
		}
		this.pictureAsking = true;
		try {
			const urls = await Promise.all(wanted.map((picture) => this.#picture(picture, view)));
			if (mine !== this.#momentsAsked) return;
			this.shown = {
				moment,
				pictures: wanted.map((picture, index) => ({ ...picture, url: urls[index] }))
			};
			this.pictureProblem = undefined;
		} catch (error) {
			if (mine !== this.#momentsAsked) return;
			// The moment before stays on the map whole, under its own time.
			this.pictureProblem = inWords(error);
		} finally {
			if (mine === this.#momentsAsked) this.pictureAsking = false;
		}
	}

	/**
	 * Goes through the moments once, the oldest first, four a second, and
	 * stops at the newest. Every picture is fetched before the first step.
	 */
	async play(): Promise<void> {
		const view = this.view;
		if (!view || view.moments.length < 2 || this.playing) return;
		const mine = ++this.#plays;
		this.playing = true;
		await Promise.allSettled(
			view.moments.flatMap((moment) =>
				drawn(view, moment).map((picture) => this.#picture(picture, view))
			)
		);
		for (const [index] of view.moments.entries()) {
			if (mine !== this.#plays) return;
			this.chosenTimeUnixS = chosenTime(view, index);
			if (index < view.moments.length - 1) await this.#wait(PLAY_MS);
		}
		if (mine === this.#plays) this.playing = false;
	}

	/** Ends a play where it is. */
	stop(): void {
		this.#plays += 1;
		this.playing = false;
	}

	/**
	 * Keeps the layer current while it is on and the map in front: asks the
	 * server for the list at once if it is due and then every five minutes,
	 * says the frames' ages again every minute in between, and asks at once
	 * when the tab comes back to the front. Returns the stop.
	 */
	keepCurrent(): () => void {
		const look = () => {
			if (!inFront()) return;
			if (isDue(this.#askedMs, this.#clock())) void this.load();
			else void this.#sayAgesAgain();
		};
		look();
		const timer = setInterval(look, AGES_EVERY_MS);
		const back = () => {
			if (inFront()) void this.load();
		};
		globalThis.document?.addEventListener('visibilitychange', back);
		return () => {
			clearInterval(timer);
			globalThis.document?.removeEventListener('visibilitychange', back);
			this.stop();
		};
	}

	/** Asks the server for the list now. A list that was read stays when this fails. */
	async load(): Promise<void> {
		if (this.asking) return;
		this.asking = true;
		this.#askedMs = this.#clock();
		try {
			const read = await this.#core.radar();
			if (read) {
				this.#take(read);
				this.problem = undefined;
			} else {
				this.problem = WORDS.noServer;
			}
		} catch (error) {
			this.problem = inWords(error);
			// Its frames go on ageing, and say so.
			await this.#sayAgesAgain();
		} finally {
			this.asking = false;
		}
	}

	async #sayAgesAgain(): Promise<void> {
		try {
			const read = await this.#core.radarAgain();
			if (read) this.#take(read);
		} catch {
			// The list that was read stays as it was worded.
		}
	}

	#take(read: RadarView): void {
		this.view = read;
		// A picture no moment of the list draws is not drawn again: it is let go.
		const held = new Set(read.moments.flatMap((moment) => drawn(read, moment).map(pictureKey)));
		for (const key of this.#pictures.keys()) {
			if (!held.has(key)) this.#pictures.delete(key);
		}
	}

	#picture(wanted: Pictured, view: RadarView): Promise<string> {
		const key = pictureKey(wanted);
		let picture = this.#pictures.get(key);
		if (!picture) {
			picture = this.#core
				.radarPicture(wanted.path, wanted.above, colours(view.key, wanted.drawsUnseen))
				.then(pictureAddress);
			this.#pictures.set(key, picture);
			// A failure is not kept: "Try again" asks again.
			picture.catch(() => {
				if (this.#pictures.get(key) === picture) this.#pictures.delete(key);
			});
		}
		return picture;
	}
}

/** An error in words for the pilot: the core's as they are, anything else behind the shell's. */
function inWords(error: unknown): string {
	return inWordsOr(WORDS.unreachable, error);
}
