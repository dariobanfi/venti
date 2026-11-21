/**
 * "Programs with access", as the worker talks to the server about it: the
 * tokens a pilot gives a logger, a script or another maker's app, so that
 * it can act for them without their password.
 *
 * Everything a pilot reads is the core's (`crates/core/src/tokens.rs`): the
 * screen's and the form's words, the check of a name, the requests, what
 * the answers mean and every refusal. This file sends the requests in
 * between, with the session token, as the account's other requests go.
 *
 * A token's secret passes through here once, from the server's answer to
 * the page that shows it. Nothing here keeps it, and nothing writes it
 * anywhere.
 */

import { type Send, sendToThisOrigin } from './account-source';
import {
	type TokenCreated,
	tokenCreated,
	tokenCreateRequest,
	type TokenForm,
	tokenForm,
	tokenNameCheck,
	tokens,
	tokensRequest,
	type TokenView,
	tokenWithdrawal,
	type TokenWithdrawal,
	tokenWithdrawn,
	tokenWithdrawRequest
} from './pkg/venti_core_wasm.js';
import type { CoreSession } from './session';

/** What a pilot enters to give a program access. */
export interface AccessWanted {
	name: string;
	/** The names of the scopes chosen, as the form's list has them. */
	scopes: string[];
	/** The moment from which the token is no longer good, if it has an end. */
	endsUnixS: number | undefined;
	/** The account's password: the server asks for it before it makes a token. */
	password: string;
}

/** A program in the list, with the words that ask before its access is withdrawn. */
export interface ProgramView {
	token: TokenView;
	withdrawal: TokenWithdrawal;
}

export class TokensSource {
	#session: CoreSession;
	#send: Send;
	#now: () => number;

	constructor(session: CoreSession, send: Send = sendToThisOrigin, now: () => number = Date.now) {
		this.#session = session;
		this.#send = send;
		this.#now = now;
	}

	/** The words of the screen and of the form that gives access. */
	form(): TokenForm {
		return tokenForm();
	}

	/**
	 * The name of a program as it will be kept, without the spaces around
	 * it. Throws, in the core's words, when the server would refuse it.
	 */
	checkName(name: string): string {
		return tokenNameCheck(name);
	}

	/** The programs that have access to the account, the newest first. */
	async list(): Promise<ProgramView[]> {
		const answer = await this.#send(tokensRequest(), await this.#session.token());
		return tokens(answer.status, answer.body, this.#nowUnixS()).map(listed);
	}

	/**
	 * Gives a program access. The answer holds the token's secret, this
	 * once. Throws, in the core's words, when the form cannot be sent as it
	 * is, before the server is asked, and when the server refuses.
	 */
	async give(wanted: AccessWanted): Promise<TokenCreated> {
		const request = tokenCreateRequest(
			wanted.name,
			wanted.scopes,
			wanted.endsUnixS,
			wanted.password
		);
		const answer = await this.#send(request, await this.#session.token());
		return tokenCreated(answer.status, answer.body, this.#nowUnixS());
	}

	/** Withdraws a program's access, by the identifier the list gives. */
	async withdraw(id: string): Promise<void> {
		const answer = await this.#send(tokenWithdrawRequest(id), await this.#session.token());
		tokenWithdrawn(answer.status, answer.body);
	}

	#nowUnixS(): number {
		return Math.floor(this.#now() / 1000);
	}
}

function listed(token: TokenView): ProgramView {
	return { token, withdrawal: tokenWithdrawal(token.name) };
}
