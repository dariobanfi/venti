/**
 * Looking for a pilot by name: what a search field is shown while the
 * pilot types. The core says what to ask the server and reads its answer
 * into lines; it also refuses, in words, a text too short to ask for.
 * This waits until the typing rests, since the server counts every
 * question against the address it comes from, and drops the answer to a
 * question that is no longer the one asked.
 */

import type { PilotsFound } from '#lib/core/index.ts';
import { words } from './pages';

/** What a search field shows under itself. */
export interface SearchState {
	/** The text looked for, as typed. Empty when nothing is. */
	text: string;
	/** Whether the answer to `text` is still to come. */
	waiting: boolean;
	/** The pilots found for `text`, with the core's line for how many. */
	found: PilotsFound | undefined;
	/** Why there is no answer, in words: the core's for a text too short, or the server's. */
	problem: string | undefined;
}

export const NOT_SEARCHED: SearchState = {
	text: '',
	waiting: false,
	found: undefined,
	problem: undefined
};

/** How long the typing has to rest before the server is asked, in milliseconds. */
export const TYPING_REST_MS = 350;

export class PilotSearch {
	#ask: (text: string) => Promise<PilotsFound | undefined>;
	#tell: (state: SearchState) => void;
	#restMs: number;
	#timer: ReturnType<typeof setTimeout> | undefined;
	/** The text last looked for. */
	#text = '';

	constructor(
		ask: (text: string) => Promise<PilotsFound | undefined>,
		tell: (state: SearchState) => void,
		restMs = TYPING_REST_MS
	) {
		this.#ask = ask;
		this.#tell = tell;
		this.#restMs = restMs;
	}

	/**
	 * Looks for the pilots of a text once the typing has rested. An empty
	 * text clears the field's answer at once and asks nothing.
	 */
	look(typed: string): void {
		const text = typed.trim();
		if (text === this.#text) return;
		this.#text = text;
		clearTimeout(this.#timer);
		if (!text) {
			this.#tell(NOT_SEARCHED);
			return;
		}
		this.#tell({ text, waiting: true, found: undefined, problem: undefined });
		this.#timer = setTimeout(() => void this.#find(text), this.#restMs);
	}

	async #find(text: string): Promise<void> {
		let state: SearchState;
		try {
			const found = await this.#ask(text);
			// A site without Venti behind it has nobody to find, and no field.
			state = { text, waiting: false, found, problem: undefined };
		} catch (error) {
			state = { text, waiting: false, found: undefined, problem: words(error) };
		}
		// Something else has been typed since.
		if (text === this.#text) this.#tell(state);
	}
}
