/**
 * The rain radar, as the worker fetches it: the frames the Venti server
 * holds of what a weather service's radars saw, and each frame's picture.
 *
 * What to ask and how to read the answer are the core's (`radarPath`,
 * `radar`): every frame's time and age in words, when the newest is too
 * old to be the rain of now, the key and the notes. A picture's pixels are
 * steps of rain, not colours; the core writes the app's colours into it,
 * and makes it clear wherever a picture drawn over it sees the place
 * (`radarFrameImageUnder`). This file sends the requests in between and
 * tells the core the moment by Venti's clock and how far the pilot's clock
 * is ahead of UTC. It keeps the list last fetched, so that the ages can be
 * said again as time goes by without asking the server, and the pictures
 * of its frames as the server sent them, which one is coloured under.
 */

import { radar, radarFrameImageUnder, radarPath, type RadarView } from './pkg/venti_core_wasm.js';
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
	/** The pictures as the server sent them, by their paths: those of the frames of the newest list. */
	#sent = new Map<string, Promise<Answer>>();

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
		// A frame the server no longer lists is not drawn again: its picture is let go.
		const listed = new Set(read.layers.flatMap((layer) => layer.frames.map((frame) => frame.path)));
		for (const path of this.#sent.keys()) {
			if (!listed.has(path)) this.#sent.delete(path);
		}
		return read;
	}

	/**
	 * The list last fetched, read again at this moment: the same frames,
	 * each as old as it is now. Nothing before a list was fetched.
	 */
	async again(): Promise<RadarView | undefined> {
		return this.#list && this.#read(this.#list);
	}

	/**
	 * A frame's picture, by the frame's path, in the colours given: a PNG.
	 * `above` are the paths of the frames drawn over it at the moment, the
	 * lowest first: the picture is clear wherever one of them sees the
	 * place, rain or dry. None for the topmost.
	 */
	async picture(path: string, above: string[], colours: RadarColours): Promise<Uint8Array> {
		const [answer, ...over] = await Promise.all([path, ...above].map((each) => this.#asSent(each)));
		// A refusal has no picture in it, which the core says in its words.
		return radarFrameImageUnder(
			answer.body,
			over.map((each) => each.body),
			new Uint32Array(colours.rain),
			colours.unseen
		);
	}

	/**
	 * A frame's picture as the server sent it, asked for once: the same
	 * frame is coloured for itself and lies over another's.
	 */
	#asSent(path: string): Promise<Answer> {
		let sent = this.#sent.get(path);
		if (!sent) {
			const asked = this.#get(path);
			sent = asked;
			this.#sent.set(path, asked);
			// Only a picture is kept: a refusal, or no answer, is asked for again.
			const letGo = () => {
				if (this.#sent.get(path) === asked) this.#sent.delete(path);
			};
			asked.then((answer) => {
				if (answer.status !== 200) letGo();
			}, letGo);
		}
		return sent;
	}

	async #read(answer: Answer): Promise<RadarView> {
		const nowUnixS = await this.#now();
		const offsetS = -new Date(nowUnixS * 1000).getTimezoneOffset() * 60;
		return radar(answer.status, answer.body, offsetS, nowUnixS);
	}
}
