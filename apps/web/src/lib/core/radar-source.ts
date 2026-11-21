/**
 * The rain radar, as the worker fetches it: the frames the Venti server
 * holds of what a weather service's radars saw, and each frame's picture.
 *
 * What to ask and how to read the answer are the core's (`radarPath`,
 * `radar`): every frame's time and age in words, when the newest is too
 * old to be the rain of now, the key and the notes. A picture's pixels are
 * steps of rain, not colours; the core writes the app's colours into it
 * (`radarFrameImage`). This file sends the requests in between and tells
 * the core the moment by Venti's clock and how far the pilot's clock is
 * ahead of UTC. It keeps the list last fetched, so that the ages can be
 * said again as time goes by without asking the server.
 */

import { radar, radarFrameImage, radarPath, type RadarView } from './pkg/venti_core_wasm.js';
import { unreachable } from './protocol';
import type { Answer, Get } from './wind-source';

/**
 * The colours a picture is written in, each as `0xRRGGBBAA`: the six steps
 * of rain, the first step first, and what the radar did not see.
 */
export interface RadarColours {
	rain: number[];
	unseen: number;
}

/** The shell's words for a server that gave no answer at all. */
export const RADAR_UNREACHABLE = 'Venti could not be reached for the rain radar.';

/** How long a request may take before the server counts as out of reach, in milliseconds. */
const PATIENCE_MS = 20_000;

/**
 * Fetches from the origin this site is served from, with the browser's own
 * cache: the list may be reused for a minute, a frame's picture for a day.
 */
const getRadar: Get = async (path) => {
	try {
		const response = await fetch(path, { signal: AbortSignal.timeout(PATIENCE_MS) });
		return { status: response.status, body: new Uint8Array(await response.arrayBuffer()) };
	} catch {
		throw unreachable(RADAR_UNREACHABLE);
	}
};

export class RadarSource {
	#present: () => Promise<boolean>;
	#now: () => Promise<number>;
	#get: Get;
	/** The newest list the core could read. */
	#list: Answer | undefined;

	constructor(
		present: () => Promise<boolean> = async () => true,
		now: () => Promise<number> = async () => Date.now() / 1000,
		get: Get = getRadar
	) {
		this.#present = present;
		this.#now = now;
		this.#get = get;
	}

	/**
	 * The radar's layers and their frames as the server has them now.
	 * Nothing if this site has no server. Throws, in words for the pilot,
	 * when the server gives no usable answer, and when it has no radar.
	 */
	async radar(): Promise<RadarView | undefined> {
		if (!(await this.#present())) return undefined;
		const answer = await this.#get(radarPath());
		const read = await this.#read(answer);
		this.#list = answer;
		return read;
	}

	/**
	 * The list last fetched, read again at this moment: the same frames,
	 * each as old as it is now. Nothing before a list was fetched.
	 */
	async again(): Promise<RadarView | undefined> {
		return this.#list && this.#read(this.#list);
	}

	/** A frame's picture, by the frame's path, in the colours given: a PNG. */
	async picture(path: string, colours: RadarColours): Promise<Uint8Array> {
		const answer = await this.#get(path);
		// A refusal has no picture in it, which the core says in its words.
		return radarFrameImage(answer.body, new Uint32Array(colours.rain), colours.unseen);
	}

	async #read(answer: Answer): Promise<RadarView> {
		const nowUnixS = await this.#now();
		const offsetS = -new Date(nowUnixS * 1000).getTimezoneOffset() * 60;
		return radar(answer.status, answer.body, offsetS, nowUnixS);
	}
}
