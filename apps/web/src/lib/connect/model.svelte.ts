/**
 * The page `/connect/` as the screen sees it: a program's request to read
 * the pilot's flights, in the state it is in, and what became of the
 * pilot's answer.
 *
 * Every word and every state is the core's (`crates/core/src/agreement.rs`);
 * the worker sends its two requests (`core/agreement-source.ts`). This
 * class keeps what the screen draws, ends the page by itself at the
 * request's end, and sends the browser on when an answer was taken.
 *
 * The identifier of the request is held here, in memory, for as long as
 * the page is open. The address the browser is sent to after an answer is
 * held nowhere: it is handed to the browser at once. Neither is logged,
 * stored or put into an error.
 */

import type { AgreementPage, AgreementWords, CoreClient } from '#lib/core/index.ts';

/** What the page asks of the core. */
export type AgreementCore = Pick<CoreClient, 'agreementSaid' | 'agreementRead' | 'agreementAnswer'>;

/** The longest a browser's timer waits, in milliseconds. */
const LONGEST_TIMER_MS = 2 ** 31 - 1;

export class ConnectModel {
	/** The words of the buttons. Nothing until the core has answered. */
	words = $state.raw<AgreementWords>();

	/** The page to show. Nothing while the request is read for the first time. */
	page = $state.raw<AgreementPage>();

	/** Why the last answer was not taken, in the core's words, while the request still waits. */
	said = $state.raw<string>();

	/** Which of the two answers is on its way, if one is. */
	answering = $state<'allow' | 'refuse'>();

	/** Whether the request is being read. */
	reading = $state(false);

	#core: AgreementCore;
	#id: string;
	#leave: (to: string) => void;
	#signedOut: () => void;
	/** The page of a request that has ended, once the core has said it. */
	#ended: AgreementPage | undefined;
	/** The question last asked: an answer to an older one is let go. */
	#asked = 0;
	#timer: ReturnType<typeof setTimeout> | undefined;

	/**
	 * `id` is the identifier in the page's address. `signedOut` is called
	 * when the server no longer takes this browser's session. `leave` sends
	 * the browser to the program's address in place of this page.
	 */
	constructor(
		core: AgreementCore,
		id: string,
		signedOut: () => void = () => {},
		leave: (to: string) => void = (to) => location.replace(to)
	) {
		this.#core = core;
		this.#id = id;
		this.#signedOut = signedOut;
		this.#leave = leave;
	}

	/** Reads the request: when the page opens, when the pilot has signed in, and on "Try again". */
	async read(): Promise<void> {
		const asked = ++this.#asked;
		this.reading = true;
		try {
			const [said, read] = await Promise.all([
				this.#core.agreementSaid(),
				this.#core.agreementRead(this.#id)
			]);
			if (asked !== this.#asked) return;
			this.words = said.words;
			this.#ended = said.ended;
			// What an answer said of signing in stays until the pilot has.
			if (read.page.state !== 'signInFirst') this.said = undefined;
			this.#show(read.page, read.nowUnixS);
		} catch {
			// The core gave no answer: the page stays as it is.
		} finally {
			if (asked === this.#asked) this.reading = false;
		}
	}

	/**
	 * Answers the request: `agree` is "Allow". An answer the server took
	 * sends the browser to the program at once; one it did not take is said
	 * in the core's words.
	 */
	async answer(agree: boolean): Promise<void> {
		if (this.answering || this.page?.state !== 'asking') return;
		const asked = this.#asked;
		this.answering = agree ? 'allow' : 'refuse';
		this.said = undefined;
		let left = false;
		try {
			const outcome = await this.#core.agreementAnswer(this.#id, agree);
			if (outcome.redirectTo !== undefined) {
				// A secret for two minutes: to the browser, and nowhere else.
				left = true;
				this.#leave(outcome.redirectTo);
				return;
			}
			// The page ended or was read anew while the answer was on its way.
			if (asked !== this.#asked || !this.page) return;
			if (outcome.state === 'asking') {
				// The request still waits: the pilot sets it right and answers again.
				this.said = outcome.message;
			} else if (outcome.state === 'signInFirst') {
				this.said = outcome.message;
				this.page = { ...this.page, state: 'signInFirst', account: undefined };
				this.#signedOut();
			} else {
				this.#show(
					{
						state: outcome.state,
						title: outcome.title,
						message: outcome.message,
						mayReadLabel: '',
						mayRead: [],
						rows: [],
						warning: undefined,
						account: undefined,
						withdraw: '',
						endsUnixS: 0
					},
					0
				);
			}
		} catch {
			// The core gave no answer: the request still waits.
		} finally {
			// While the browser leaves, the buttons stay as they are.
			if (!left) this.answering = undefined;
		}
	}

	/** Stops the page's clock: the page is left. */
	close(): void {
		this.#asked += 1;
		clearTimeout(this.#timer);
		this.#timer = undefined;
	}

	/**
	 * Shows a page, and ends it by itself at the request's end. The time
	 * until then is counted from the server's clock as it was read with the
	 * request; while that is not known the page waits for the server to say.
	 */
	#show(page: AgreementPage, nowUnixS: number): void {
		clearTimeout(this.#timer);
		this.#timer = undefined;
		this.page = page;
		const waits = page.state === 'asking' || page.state === 'signInFirst';
		if (!waits || page.endsUnixS <= 0 || nowUnixS <= 0) return;
		const leftMs = Math.max(0, (page.endsUnixS - nowUnixS) * 1000);
		this.#timer = setTimeout(() => this.#end(), Math.min(leftMs, LONGEST_TIMER_MS));
	}

	#end(): void {
		this.#timer = undefined;
		if (!this.#ended) return;
		// An answer to a question asked before the end is let go.
		this.#asked += 1;
		this.reading = false;
		this.said = undefined;
		this.page = this.#ended;
	}
}
