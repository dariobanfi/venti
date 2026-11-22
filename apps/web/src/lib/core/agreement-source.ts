/**
 * Letting a program read the pilot's flights, as the worker talks to the
 * server about it: the page `/connect/`, on which a pilot who is signed in
 * here answers a program's request without typing a password.
 *
 * Every word the page shows, the two requests and what their answers mean
 * are the core's (`crates/core/src/agreement.rs`). This file sends the
 * requests in between: the one that reads a request without a token, since
 * whoever holds the identifier may read it, and the one that answers it
 * with the session's token and nothing else.
 *
 * The identifier of a request and the address the browser is sent to after
 * an answer pass through here once. Nothing here keeps either, and nothing
 * writes either anywhere: no error names them, and nothing is logged.
 */

import { type Send, sendToThisOrigin } from './account-source';
import {
	agreementAnswered,
	agreementAnswerRequest,
	agreementEnded,
	type AgreementOutcome,
	agreementPage,
	type AgreementPage,
	agreementRequest,
	agreementWords,
	type AgreementWords,
	type ServerRequest
} from './pkg/venti_core_wasm.js';
import type { CoreSession } from './session';

/** What the page says whatever the request: its buttons, and the page of a request that has ended. */
export interface AgreementSaid {
	words: AgreementWords;
	ended: AgreementPage;
}

/** A request as it was read, and the moment it was read at. */
export interface AgreementRead {
	page: AgreementPage;
	/**
	 * Now by the server's clock, seconds since 1970; 0 while the app has not
	 * read it. The page counts the time to the request's end from it, never
	 * from this device's clock.
	 */
	nowUnixS: number;
}

/** Why the server gave no answer at all, in words for the pilot. */
function said(error: unknown): string {
	return error instanceof Error ? error.message : '';
}

export class AgreementSource {
	#session: CoreSession;
	#now: () => Promise<number>;
	#send: Send;

	constructor(
		session: CoreSession,
		now: () => Promise<number> = async () => 0,
		send: Send = sendToThisOrigin
	) {
		this.#session = session;
		this.#now = now;
		this.#send = send;
	}

	/** The words of the page that do not depend on the request. */
	said(): AgreementSaid {
		return { words: agreementWords(), ended: agreementEnded() };
	}

	/**
	 * Reads the request the page's address names, for the account this
	 * browser is signed in to, or for nobody. Text that is no identifier is
	 * the page of a request that has ended, and nothing is sent. It never
	 * fails: what went wrong is a page too.
	 */
	async read(id: string): Promise<AgreementRead> {
		let request: ServerRequest;
		try {
			request = agreementRequest(id);
		} catch {
			return { page: agreementEnded(), nowUnixS: 0 };
		}
		const account = await this.#session.ask((core) => core.account());
		const handle = account && !account.needsSignIn ? account.handle : undefined;
		try {
			const answer = await this.#send(request, undefined);
			return {
				page: agreementPage(answer.status, answer.body, handle),
				nowUnixS: await this.#now().catch(() => 0)
			};
		} catch (error) {
			// No answer at all: the core's page for a server that could not be
			// asked, with what the app says of a server out of reach.
			return {
				page: { ...agreementPage(0, new Uint8Array(), handle), message: said(error) },
				nowUnixS: 0
			};
		}
	}

	/**
	 * Answers the request: `agree` is "Allow". It goes with the session's
	 * token. What comes back says where the browser goes, or what the page
	 * shows in its place. It never fails.
	 */
	async answer(id: string, agree: boolean): Promise<AgreementOutcome> {
		let request: ServerRequest;
		try {
			request = agreementAnswerRequest(id, agree);
		} catch {
			const ended = agreementEnded();
			return {
				redirectTo: undefined,
				state: ended.state,
				title: ended.title,
				message: ended.message
			};
		}
		try {
			const answer = await this.#send(request, await this.#session.token());
			return agreementAnswered(answer.status, answer.body);
		} catch (error) {
			// No answer at all: the request still waits, and the pilot answers again.
			return { ...agreementAnswered(0, new Uint8Array()), message: said(error) };
		}
	}
}
