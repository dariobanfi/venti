/**
 * The pilot's account on the Venti server, as the worker talks to it.
 *
 * What to ask the server and what its answers mean is the core's: it hands
 * out a request and reads the answer. This file sends the requests in
 * between, with the session token where a request needs it and nowhere
 * else, and runs the sync that keeps the logbook and the server in step. It
 * is the browser's counterpart of the iPhone app's `AccountModel.swift`.
 *
 * The server is the one this site is served from, as for the wind.
 *
 * The session token never reaches the page: it is read from the store
 * here, in the worker, for the one request that needs it. That does not
 * make it a secret from the site's own scripts, which can open the same
 * store; `apps/web/README.md` says what was weighed.
 */

import {
	type AccountView,
	type FlightSharing,
	type FlightVisibility,
	type ServerRequest,
	type SyncSummary,
	syncSummaryInWords
} from './pkg/venti_core_wasm.js';
import { UNREACHABLE } from './protocol';
import type { CoreSession } from './session';
import { StaleStoreError } from './store';

/** What the server answered. */
export interface ServerAnswer {
	status: number;
	body: Uint8Array;
	/** The answer's `Retry-After` header in seconds, if it had one. */
	retryAfterS: number | undefined;
}

/** Sends one of the core's requests to the server. Throws when no answer came at all. */
export type Send = (request: ServerRequest, token: string | undefined) => Promise<ServerAnswer>;

/** How a sync ended. */
export type SyncOutcome =
	| {
			kind: 'finished';
			summary: SyncSummary;
			/** What it did, in the core's words: "3 sent, 1 fetched". */
			said: string;
	  }
	| {
			kind: 'failed';
			/** Why it stopped, in words for the pilot. What was done so far is kept. */
			message: string;
			/** Whether the session has ended and the pilot has to sign in again. */
			needsSignIn: boolean;
	  };

/** What a pilot enters to create an account. */
export interface NewAccount {
	handle: string;
	displayName: string;
	password: string;
	/** An invite code, on a server that asks for one. */
	invite: string;
}

/** How long a request may take before the server counts as out of reach, in milliseconds. */
const PATIENCE_MS = 120_000;

/** How often a sync starts over when the store keeps changing under it. */
const SYNC_ATTEMPTS = 3;

/**
 * Sends a request to the origin this site is served from. Nothing is kept
 * of the answer, and nothing the browser holds for the site goes along by
 * itself: an account is acted for with the token in the header and in no
 * other way.
 */
export const sendToThisOrigin: Send = async (request, token) => {
	const headers: Record<string, string> = { accept: 'application/json' };
	if (request.contentType) headers['content-type'] = request.contentType;
	if (request.needsToken && token) headers.authorization = `Bearer ${token}`;
	try {
		const response = await fetch(request.path, {
			method: request.method,
			headers,
			body: request.body as Uint8Array<ArrayBuffer> | undefined,
			cache: 'no-store',
			credentials: 'omit',
			// A redirect could carry a password or a flight somewhere else.
			redirect: 'error',
			referrerPolicy: 'no-referrer',
			signal: AbortSignal.timeout(PATIENCE_MS)
		});
		return {
			status: response.status,
			body: new Uint8Array(await response.arrayBuffer()),
			retryAfterS: wholeSeconds(response.headers.get('retry-after'))
		};
	} catch {
		// The browser says "Failed to fetch" for every cause; say what it means.
		const error = new Error('The server could not be reached.');
		error.name = UNREACHABLE;
		throw error;
	}
};

/** A `Retry-After` header given in seconds. One given as a date is passed over. */
export function wholeSeconds(header: string | null): number | undefined {
	return header !== null && /^\d{1,9}$/.test(header.trim()) ? Number(header.trim()) : undefined;
}

/**
 * What this browser is called in the account's list of devices: "Firefox on
 * Linux". It is read from what the browser says about itself, which is a
 * guess at best and only ever shown to the pilot.
 */
export function deviceName(agent: string): string {
	const browser = (
		[
			[/\bEdg(?:e|A|iOS)?\//, 'Edge'],
			[/\bOPR\//, 'Opera'],
			[/\b(?:Firefox|FxiOS)\//, 'Firefox'],
			[/\b(?:Chrome|CriOS)\//, 'Chrome'],
			[/\bSafari\//, 'Safari']
		] as const
	).find(([pattern]) => pattern.test(agent))?.[1];
	const system = (
		[
			[/\biPhone\b/, 'iPhone'],
			[/\biPad\b/, 'iPad'],
			[/\bAndroid\b/, 'Android'],
			[/\bWindows\b/, 'Windows'],
			[/\bMac OS X\b|\bMacintosh\b/, 'Mac'],
			[/\bCrOS\b/, 'ChromeOS'],
			[/\bLinux\b/, 'Linux']
		] as const
	).find(([pattern]) => pattern.test(agent))?.[1];
	if (browser && system) return `${browser} on ${system}`;
	return browser ?? (system ? `Browser on ${system}` : 'Web browser');
}

export class AccountSource {
	#session: CoreSession;
	#send: Send;
	/** The address the requests go to, as the core remembers the account by it. */
	#server: string;
	#device: string;
	#now: () => number;

	constructor(
		session: CoreSession,
		send: Send = sendToThisOrigin,
		server: string = location.origin,
		device: string = deviceName(navigator.userAgent),
		now: () => number = Date.now
	) {
		this.#session = session;
		this.#send = send;
		this.#server = server;
		this.#device = device;
		this.#now = now;
	}

	/** The account this browser is signed in to, if any. */
	account(): Promise<AccountView | undefined> {
		return this.#session.ask((core) => core.account());
	}

	/** Signs in. Throws, in the core's words, when the server refuses. */
	async signIn(handle: string, password: string): Promise<AccountView> {
		const request = await this.#session.ask((core) =>
			core.signInRequest(handle, password, this.#device)
		);
		return this.#enter(request);
	}

	/** Creates an account and signs in. */
	async createAccount(wanted: NewAccount): Promise<AccountView> {
		const request = await this.#session.ask((core) =>
			core.createAccountRequest(
				wanted.handle,
				wanted.displayName,
				wanted.password,
				wanted.invite,
				this.#device
			)
		);
		return this.#enter(request);
	}

	async #enter(request: ServerRequest): Promise<AccountView> {
		const answer = await this.#send(request, undefined);
		// The account and its token are stored together or not at all.
		const signedIn = await this.#session.change(
			(core) => core.signedIn(this.#server, answer.status, answer.body),
			(signedIn) => signedIn.token
		);
		return signedIn.account;
	}

	/**
	 * Signs this browser out. The server is told if it can be reached; the
	 * browser forgets the session either way. The flights in the logbook stay.
	 */
	async signOut(): Promise<void> {
		try {
			const request = await this.#session.ask((core) => core.signOutRequest());
			if (request) await this.#send(request, await this.#session.token());
		} catch {
			// Out of reach: the session ends on the server when it expires.
		}
		await this.#session.change(
			(core) => core.signedOut(),
			() => null
		);
	}

	/**
	 * Deletes the account on the server, with its flights there. The logbook
	 * in this browser stays. Throws, in the core's words, when the server
	 * refuses, as it does for a wrong password.
	 */
	async deleteAccount(password: string): Promise<void> {
		const request = await this.#session.ask((core) => core.deleteAccountRequest(password));
		const answer = await this.#send(request, await this.#session.token());
		await this.#session.change(
			(core) => core.accountDeleted(answer.status, answer.body),
			() => null
		);
	}

	/** How a flight is shared, if the server has it. */
	sharing(id: string): Promise<FlightSharing | undefined> {
		return this.#session.ask((core) => core.flightSharing(id));
	}

	/** Changes who may see a flight, and says how it is shared now. */
	async share(id: string, visibility: FlightVisibility): Promise<FlightSharing> {
		const request = await this.#session.ask((core) => core.shareRequest(id, visibility));
		const answer = await this.#send(request, await this.#session.token());
		return this.#session.change((core) => core.shared(id, answer.status, answer.body));
	}

	/**
	 * Brings the logbook and the server in step, one of the core's steps
	 * after another. Throws when the server cannot be reached; what was done
	 * until then is kept, and the next sync goes on from there.
	 *
	 * Only one tab of the site syncs at a time. A second one waits and then
	 * finds the work done.
	 */
	sync(): Promise<SyncOutcome> {
		const locks = globalThis.navigator?.locks;
		return locks ? locks.request('venti-sync', () => this.#sync()) : this.#sync();
	}

	async #sync(): Promise<SyncOutcome> {
		for (let attempt = 1; ; attempt += 1) {
			const run = await this.#session.beginSync();
			try {
				let step = await run.start(Math.floor(this.#now() / 1000));
				while (step.kind === 'send') {
					const answer = await this.#send(step.request, await this.#session.token());
					step = await run.receive(answer.status, answer.body, answer.retryAfterS);
				}
				if (step.kind === 'finished') {
					return { ...step, said: syncSummaryInWords(step.summary) };
				}
				// The server no longer takes the token: it is of no use to anyone.
				if (step.needsSignIn) await this.#session.forgetToken();
				return step;
			} catch (error) {
				// The logbook changed under the run, in another tab. Every step
				// so far is stored; start over from what is there now.
				if (!(error instanceof StaleStoreError)) throw error;
				if (attempt === SYNC_ATTEMPTS) {
					return {
						kind: 'failed',
						message: 'The logbook kept changing in another tab. Sync again in a moment.',
						needsSignIn: false
					};
				}
			} finally {
				run.end();
			}
		}
	}
}
