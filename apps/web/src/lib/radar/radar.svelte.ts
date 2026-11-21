/**
 * The rain radar as the map tab sees it: asks the core in its worker for
 * the list of frames and for each frame's picture, keeps the pictures of
 * this visit, and remembers which frame the pilot looks at.
 *
 * What is on the map is always a picture with its own time: while another
 * frame's picture is asked for, or cannot be had, the one before stays, and
 * the card names that one. `radar.ts` holds the rules.
 */

import type { CoreClient, RadarView } from '#lib/core/index.ts';
import { isInWords } from '#lib/core/index.ts';
import {
	AGES_EVERY_MS,
	chosenTime,
	colours,
	corners,
	frameOf,
	isDue,
	layerOf,
	pictureAddress,
	PLAY_MS,
	WORDS
} from './radar';

type Core = Pick<CoreClient, 'radar' | 'radarAgain' | 'radarPicture'>;

const inFront = () => (globalThis.document?.visibilityState ?? 'visible') === 'visible';

const pause = (ms: number) => new Promise<void>((done) => setTimeout(done, ms));

export class RadarModel {
	/** The newest list read in this visit. */
	view = $state.raw<RadarView>();
	/** Whether the server is being asked for the list. */
	asking = $state(false);
	/** Why the last asking brought no list, in words for the pilot. */
	problem = $state<string>();
	/** The time of the frame the pilot chose; nothing for the newest. */
	chosenTimeUnixS = $state<number>();
	/** The picture on the map, by its frame's path. */
	shown = $state.raw<{ path: string; url: string }>();
	/** Whether the chosen frame's picture is being asked for. */
	pictureAsking = $state(false);
	/** Why the chosen frame's picture could not be had, in words. */
	pictureProblem = $state<string>();
	/** Whether the frames are being played. */
	playing = $state(false);
	/** Whether the pilot unfolded the card's key and notes: folded when the app opens. */
	keyOpen = $state(false);

	/** The layer shown. */
	readonly layer = $derived(layerOf(this.view));
	/** The frame the strip marks: the pilot's, or the newest. */
	readonly frame = $derived(frameOf(this.layer, this.chosenTimeUnixS));
	/** The frame whose picture is on the map, as the newest list words it. */
	readonly shownFrame = $derived(
		this.layer?.frames.find((frame) => frame.path === this.shown?.path)
	);
	/** What the map draws: the picture, its corners, and whether it is too old to be the rain of now. */
	readonly onMap = $derived(
		this.layer && this.shown && this.shownFrame
			? { url: this.shown.url, corners: corners(this.layer), faded: this.layer.stale }
			: undefined
	);

	#core: Core;
	#clock: () => number;
	#wait: (ms: number) => Promise<void>;
	#askedMs: number | undefined;
	/** The pictures fetched in this visit, by their frame's path, as addresses the map loads. */
	#pictures = new Map<string, Promise<string>>();
	/** Counts the pictures asked for and the plays, so that a slow answer is dropped. */
	#picturesAsked = 0;
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

	/** Shows the frame of a cell of the strip; choosing by hand ends a play. */
	choose(index: number): void {
		this.stop();
		if (this.layer) this.chosenTimeUnixS = chosenTime(this.layer, index);
	}

	/**
	 * Asks for the picture of the frame the strip marks, unless it is at
	 * hand, and puts it on the map. Call it again whenever that frame changes.
	 */
	async showFrame(): Promise<void> {
		const frame = this.frame;
		const view = this.view;
		const mine = ++this.#picturesAsked;
		if (!frame || !view) {
			this.pictureAsking = false;
			return;
		}
		if (this.shown?.path === frame.path) {
			this.pictureAsking = false;
			this.pictureProblem = undefined;
			return;
		}
		this.pictureAsking = true;
		try {
			const url = await this.#picture(frame.path, view);
			if (mine !== this.#picturesAsked) return;
			this.shown = { path: frame.path, url };
			this.pictureProblem = undefined;
		} catch (error) {
			if (mine !== this.#picturesAsked) return;
			// The picture before stays on the map, under its own time.
			this.pictureProblem = inWords(error);
		} finally {
			if (mine === this.#picturesAsked) this.pictureAsking = false;
		}
	}

	/**
	 * Goes through the frames once, the oldest first, four a second, and
	 * stops at the newest. Every picture is fetched before the first step.
	 */
	async play(): Promise<void> {
		const view = this.view;
		const layer = this.layer;
		if (!view || !layer || layer.frames.length < 2 || this.playing) return;
		const mine = ++this.#plays;
		this.playing = true;
		await Promise.allSettled(layer.frames.map((frame) => this.#picture(frame.path, view)));
		for (const [index] of layer.frames.entries()) {
			if (mine !== this.#plays) return;
			this.chosenTimeUnixS = chosenTime(layer, index);
			if (index < layer.frames.length - 1) await this.#wait(PLAY_MS);
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
		// A frame the server no longer holds cannot be chosen again: its picture is let go.
		const held = new Set(read.layers.flatMap((layer) => layer.frames.map((frame) => frame.path)));
		for (const path of this.#pictures.keys()) {
			if (!held.has(path)) this.#pictures.delete(path);
		}
	}

	#picture(path: string, view: RadarView): Promise<string> {
		let picture = this.#pictures.get(path);
		if (!picture) {
			picture = this.#core.radarPicture(path, colours(view.key)).then(pictureAddress);
			this.#pictures.set(path, picture);
			// A failure is not kept: "Try again" asks again.
			picture.catch(() => {
				if (this.#pictures.get(path) === picture) this.#pictures.delete(path);
			});
		}
		return picture;
	}
}

/** An error in words for the pilot: the core's as they are, anything else behind the shell's. */
function inWords(error: unknown): string {
	const said = error instanceof Error ? error.message : String(error);
	return isInWords(error) ? said : `${WORDS.unreachable} ${said}`;
}
