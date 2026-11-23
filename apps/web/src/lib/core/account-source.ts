/**
 * The pilot's account on the Venti server, as the worker talks to it.
 *
 * What to ask the server and what its answers mean is the core's: it hands
 * out a request and reads the answer. This file sends the requests in
 * between, with the session token where a request needs it and nowhere
 * else, and runs the sync of the pilot's own sites (`Core.keptSitesSync`).
 * The web keeps no logbook of its own (owner decision 20): the pilot's
 * flights are the server's, `own-flights-source.ts`. It is the browser's
 * counterpart of the iPhone app's `AccountModel.swift`.
 *
 * The server is the one this site is served from, as for the wind.
 *
 * The session token never reaches the page: it is read from the store
 * here, in the worker, for the one request that needs it. That does not
 * make it a secret from the site's own scripts, which can open the same
 * store; `apps/web/AGENTS.md` says what was weighed.
 */

import {
	type AccountView,
	type AllFlightsSet,
	type FlightVisibility,
	type Core,
	type GliderClassKind,
	type GoogleSignUp,
	type GoogleWords,
	googleWords,
	reportFormFor,
	type ReportForm,
	type ReportReason,
	reportRequest,
	reportSent,
	type ReportSubject,
	type ServerRequest,
	type SyncSummary,
	syncSummaryInWords
} from './pkg/venti_core_wasm.js';
import { unreachable } from './protocol';
import type { CoreSession, SyncRun } from './session';
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

/** How a run of the core's sync ended. */
export type RunOutcome =
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

/**
 * What a sign-in with Google came to: signed in, or a first sign-in with a
 * Google account that goes on to the handle step.
 */
export type GoogleEntered =
	{ kind: 'signedIn'; account: AccountView } | { kind: 'needsHandle'; signUp: GoogleSignUp };

/** What a pilot enters at the handle step after a first sign-in with Google. */
export interface NewGoogleAccount {
	/** What carries the Google account on, from the first answer. */
	signup: string;
	handle: string;
	displayName: string;
	/** An invite code, on a server that asks for one; empty otherwise. */
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
		throw unreachable('Venti could not be reached.');
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

	/**
	 * Asks the server what it says of the account now: its name, and
	 * whether it is a partner of Venti for flying sites, which can change
	 * while a browser stays signed in. The core remembers the answer, and
	 * what may be offered for a site follows it. The account as it is known
	 * then; as it was known before, when the server gives no usable answer.
	 * Nothing is asked in a browser that is not signed in.
	 */
	async refresh(): Promise<AccountView | undefined> {
		const request = await this.#session.ask((core) => core.accountRequest());
		if (!request) return this.account();
		try {
			const answer = await this.#send(request, await this.#session.token());
			const view = await this.#session.change((core) =>
				core.accountRead(answer.status, answer.body)
			);
			// The server no longer takes the token: it is of no use to anyone.
			if (view?.needsSignIn) await this.#session.forgetToken();
			return view;
		} catch {
			// Out of reach, or an answer the core cannot read: what is stored stays.
			return this.account();
		}
	}

	/**
	 * Sets the pilot's country, two letters such as `DE`, or clears it
	 * without one. The account as the server then says it is. Throws, in
	 * the core's words, when the server refuses.
	 */
	setCountry(country: string | undefined): Promise<AccountView | undefined> {
		return this.#changeSetting((core) => core.countryRequest(country));
	}

	/**
	 * Sets the class of the glider the pilot flies now, or clears it
	 * without one: flights sent from then on take it.
	 */
	setGliderClass(gliderClass: GliderClassKind | undefined): Promise<AccountView | undefined> {
		return this.#changeSetting((core) => core.gliderClassRequest(gliderClass));
	}

	/**
	 * Whether a flight sent to the server from now on is public from the
	 * start: the pilot's own switch, on until they switch it off.
	 */
	setNewFlightsPublic(open: boolean): Promise<AccountView | undefined> {
		return this.#changeSetting((core) => core.newFlightsPublicRequest(open));
	}

	/** Whether the pilot is among the pilots near a place, made of their public flights. */
	setShownNearPlaces(shown: boolean): Promise<AccountView | undefined> {
		return this.#changeSetting((core) => core.shownNearPlacesRequest(shown));
	}

	/**
	 * Sends a picture as the pilot's own, its bytes as they are: the server
	 * keeps a square of its own made from it. Refused in the core's words
	 * before anything is sent for a kind or a size the server would refuse.
	 */
	setPicture(bytes: Uint8Array, mediaType: string): Promise<AccountView | undefined> {
		return this.#changeSetting((core) => core.pictureRequest(bytes, mediaType));
	}

	/** Removes the pilot's picture. */
	removePicture(): Promise<AccountView | undefined> {
		return this.#changeSetting((core) => core.pictureRemovalRequest());
	}

	/**
	 * Makes every flight of the account public, or every one private, after
	 * the pilot answered the core's question (`allFlightsQuestion`). How they
	 * stand then, with the core's line. Throws, in the core's words, when
	 * the browser is not signed in or the server refuses.
	 */
	async setAllFlights(visibility: FlightVisibility): Promise<AllFlightsSet> {
		const request = await this.#session.ask((core) => core.allFlightsRequest(visibility));
		const answer = await this.#send(request, await this.#session.token());
		return this.#session.change((core) => core.allFlightsSet(answer.status, answer.body));
	}

	/** Sends a setting of the account's that the core wrote, and keeps the account it answers with. */
	async #changeSetting(write: (core: Core) => ServerRequest): Promise<AccountView | undefined> {
		const request = await this.#session.ask(write);
		const answer = await this.#send(request, await this.#session.token());
		return this.#session.change((core) => core.accountRead(answer.status, answer.body));
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

	/** The words of signing in with Google, the core's. */
	googleWords(): GoogleWords {
		return googleWords();
	}

	/**
	 * Signs in with an ID token Google gave for this site. A Google account
	 * that no Venti account is linked to yet goes on to the handle step;
	 * nothing is stored then. Throws, in the core's words, when the server
	 * refuses.
	 */
	async googleSignIn(idToken: string): Promise<GoogleEntered> {
		const request = await this.#session.ask((core) =>
			core.googleSignInRequest(idToken, this.#device)
		);
		const answer = await this.#send(request, undefined);
		// A sign-in stores the account and its token together, or not at all.
		const entered = await this.#session.change(
			(core) => core.googleSignIn(this.#server, answer.status, answer.body),
			(entered) => entered.signedIn?.token
		);
		if (entered.signedIn) return { kind: 'signedIn', account: entered.signedIn.account };
		if (entered.needsHandle) return { kind: 'needsHandle', signUp: entered.needsHandle };
		throw new Error('Venti gave an answer this browser cannot read. Try again.');
	}

	/** Creates the account of a first sign-in with Google, with the handle chosen, and signs in. */
	async createGoogleAccount(wanted: NewGoogleAccount): Promise<AccountView> {
		const request = await this.#session.ask((core) =>
			core.googleAccountRequest(
				wanted.signup,
				wanted.handle,
				wanted.displayName,
				wanted.invite,
				this.#device
			)
		);
		return this.#enter(request);
	}

	/** Links the account this browser is signed in to to the Google account of an ID token. */
	async linkGoogle(idToken: string): Promise<AccountView | undefined> {
		const request = await this.#session.ask((core) => core.linkGoogleRequest(idToken));
		const answer = await this.#send(request, await this.#session.token());
		return this.#session.change((core) => core.googleLinked(answer.status, answer.body));
	}

	/** Unlinks the account from its Google account. Refused, in words, where Google is the only way in. */
	async unlinkGoogle(): Promise<AccountView | undefined> {
		const request = await this.#session.ask((core) => core.unlinkGoogleRequest());
		const answer = await this.#send(request, await this.#session.token());
		return this.#session.change((core) => core.googleUnlinked(answer.status, answer.body));
	}

	/**
	 * Deletes an account that signs in with Google, with a fresh ID token of
	 * its Google account in place of the password. As `deleteAccount`.
	 */
	async deleteAccountWithGoogle(idToken: string): Promise<void> {
		const request = await this.#session.ask((core) => core.deleteAccountWithGoogleRequest(idToken));
		const answer = await this.#send(request, await this.#session.token());
		await this.#session.change(
			(core) => core.accountDeleted(answer.status, answer.body),
			() => null
		);
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
	 * browser forgets the session either way.
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
	 * Deletes the account on the server, with its flights. Throws, in the
	 * core's words, when the server refuses, as it does for a wrong password.
	 */
	async deleteAccount(password: string): Promise<void> {
		const request = await this.#session.ask((core) => core.deleteAccountRequest(password));
		const answer = await this.#send(request, await this.#session.token());
		await this.#session.change(
			(core) => core.accountDeleted(answer.status, answer.body),
			() => null
		);
	}

	/**
	 * What a report about such a thing as `subject` can say: the reasons
	 * with a line about each, how long the pilot's own words may be, and
	 * what happens to a report. A flight's and a group's differ. All of it
	 * is the core's.
	 */
	reportForm(subject: ReportSubject): ReportForm {
		return reportFormFor(subject);
	}

	/**
	 * Tells whoever runs the server that a flight, a pilot's page or a live
	 * track should not be there, from the account this browser is signed in
	 * to. Returns in a line what became of the report. Throws, in the core's
	 * words, when the report cannot be sent as it is (no words where they
	 * are needed, or too many), before the server is asked, and when the
	 * server refuses it.
	 */
	async report(subject: ReportSubject, reason: ReportReason, text: string): Promise<string> {
		const request = reportRequest(subject, reason, text);
		const answer = await this.#send(request, await this.#session.token());
		return reportSent(answer.status, answer.body);
	}

	/**
	 * Brings the pilot's own sites and their account in step: after a site
	 * was kept or forgotten. What the run did, or nothing when it did not
	 * come to its end. In a browser that is not signed in nothing is asked
	 * and nothing changes: the sites are this browser's alone.
	 */
	syncKeptSites(): Promise<SyncSummary | undefined> {
		return alone(() => this.#keptSites());
	}

	/** The run of the pilot's own sites. It is quiet: the list says itself where it is kept. */
	async #keptSites(): Promise<SyncSummary | undefined> {
		if ((await this.#session.token()) === undefined) return undefined;
		try {
			const outcome = await this.#run(() => this.#session.beginKeptSitesSync());
			return outcome.kind === 'finished' ? outcome.summary : undefined;
		} catch {
			// Out of reach: what was kept here waits for the next sync.
			return undefined;
		}
	}

	/** Takes a run of the core's to its end, sending each request with the session. */
	async #run(begin: () => Promise<SyncRun>): Promise<RunOutcome> {
		for (let attempt = 1; ; attempt += 1) {
			const run = await begin();
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
				// The store changed under the run, in another tab. Every step
				// so far is stored; start over from what is there now.
				if (!(error instanceof StaleStoreError)) throw error;
				if (attempt === SYNC_ATTEMPTS) {
					return {
						kind: 'failed',
						message: 'The pilot’s own sites kept changing in another tab. Sync again in a moment.',
						needsSignIn: false
					};
				}
			} finally {
				run.end();
			}
		}
	}
}

/** Runs a sync when no other tab of the site runs one. */
export function alone<T>(task: () => Promise<T>): Promise<T> {
	const locks = globalThis.navigator?.locks;
	return locks ? locks.request('venti-sync', task) : task();
}
