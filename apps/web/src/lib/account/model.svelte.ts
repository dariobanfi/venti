/**
 * The pilot's account on the Venti server, as the screens see it.
 *
 * What to ask the server and what its answers mean is the core's, and the
 * requests are sent by the worker (`core/account-source.ts`). This class
 * keeps what the screens draw, decides when the sync of the pilot's own sites runs, and puts what
 * went wrong into words. It is the browser's counterpart of the iPhone
 * app's `AccountModel.swift`.
 */

import {
	type AccessWanted,
	type AccountView,
	type AllFlightsSet,
	type CoreClient,
	type FlightVisibility,
	type GliderClassKind,
	type GoogleSignUp,
	type GoogleWords,
	isInWords,
	type NewAccount,
	type NewGoogleAccount,
	type ProgramView,
	type Registration,
	type ReportForm,
	type ReportReason,
	type ReportSubject,
	type SyncSummary,
	type TokenCreated,
	type TokenForm
} from '#lib/core/index.ts';
import { reasonFor } from '#lib/logbook/model.svelte.ts';
import type { GoogleIntent, GoogleReturn } from './google';

/** How long after a sync the next one that nobody asked for may run, in milliseconds. */
export const PAUSE_BETWEEN_SYNCS_MS = 60_000;

export class AccountModel {
	/** The account this browser is signed in to. */
	account: AccountView | undefined;

	/**
	 * Whether this site has a Venti server to sign in to. Not known before
	 * the server was asked.
	 */
	hasServer = $state<boolean>();

	/**
	 * Who may create an account on the server: anyone, whoever has an
	 * invite code, or nobody. Nothing while the server has not said, and
	 * from one that never does, which may ask for a code.
	 */
	registration = $state<Registration>();

	/**
	 * Whether the server signs pilots in with Google: whether the account
	 * screen offers Google's button. Not known before the server was asked.
	 */
	googleSignIn = $state<boolean>();

	/** The words of signing in with Google, the core's: there when `googleSignIn` is. */
	googleWords = $state.raw<GoogleWords>();

	/** A first sign-in with Google, on its way to the handle step. */
	googleStep = $state.raw<GoogleSignUp>();

	/** Why what the pilot went to Google for did not work, in words, and what it was. */
	googleRefusal = $state.raw<{ intent: GoogleIntent | undefined; words: string }>();

	/** Whether a request or a sync is under way. */
	working = $state(false);

	/** Why signing out did not work, in words. */
	problem = $state.raw<string>();

	/**
	 * How the account's flights stood after the last "Make all public" or
	 * "Make all private" here, with the core's line to show under them.
	 */
	allFlights = $state.raw<AllFlightsSet>();

	#core: CoreClient;
	#now: () => number;
	/**
	 * Called after a sync, with what it did to the pilot's own sites where
	 * their run came to its end: after signing in too.
	 */
	#onSynced: (keptSites: SyncSummary | undefined) => Promise<void>;
	#lastSyncAt: number | undefined;
	/** What a report can say, once the core was asked. */
	/** The report forms asked for, one for each kind of subject. */
	#reportForms = new Map<ReportSubject['kind'], Promise<ReportForm>>();
	/** The words of "Programs with access", once the core was asked. */
	#programsForm: Promise<TokenForm> | undefined;

	private constructor(
		core: CoreClient,
		account: AccountView | undefined,
		onSynced: (keptSites: SyncSummary | undefined) => Promise<void>,
		now: () => number
	) {
		this.#core = core;
		this.account = $state.raw(account);
		this.#onSynced = onSynced;
		this.#now = now;
	}

	/** Opens the account this browser remembers, if any. */
	static async open(
		core: CoreClient,
		onSynced: (keptSites: SyncSummary | undefined) => Promise<void> = async () => {},
		now: () => number = () => Date.now()
	): Promise<AccountModel> {
		const model = new AccountModel(core, await core.account(), onSynced, now);
		// What the server says of the account can have changed since the
		// last visit; nothing waits for the answer.
		void model.ask();
		// Whether there is a server decides only what the account screen
		// offers; nothing waits for the answer.
		void core.hasServer().then(
			(present) => (model.hasServer = present),
			() => (model.hasServer = true)
		);
		void core.registration().then(
			(registration) => (model.registration = registration),
			() => {}
		);
		// Google's button is offered with the core's words for it, or not at all.
		void Promise.all([core.googleSignIn(), core.googleWords()]).then(
			([offered, said]) => {
				model.googleWords = said;
				model.googleSignIn = offered;
			},
			() => (model.googleSignIn = false)
		);
		return model;
	}

	/** Whether the pilot is signed in and the session still holds. */
	get signedIn(): boolean {
		return this.account !== undefined && !this.account.needsSignIn;
	}

	/**
	 * Signs in, and brings the pilot's own sites in step. Returns why it
	 * did not work, in words, or nothing when it did.
	 */
	signIn(handle: string, password: string): Promise<string | undefined> {
		return this.#enter(() => this.#core.signIn(handle, password));
	}

	/** Creates an account and signs in. Returns why it did not work, if it did not. */
	createAccount(wanted: NewAccount): Promise<string | undefined> {
		return this.#enter(() => this.#core.createAccount(wanted));
	}

	/**
	 * Signs in with an ID token Google gave, and brings the pilot's own sites in step.
	 * A Google account that no Venti account is linked to yet goes on to the
	 * handle step: `signUp` says what it needs. `refusal` says, in words, why
	 * it did not work.
	 */
	async continueWithGoogle(idToken: string): Promise<{ refusal?: string; signUp?: GoogleSignUp }> {
		let signUp: GoogleSignUp | undefined;
		const refusal = await this.#enter(async () => {
			const entered = await this.#core.signInWithGoogle(idToken);
			if (entered.kind === 'signedIn') return entered.account;
			signUp = entered.signUp;
			return undefined;
		});
		return { refusal, signUp };
	}

	/**
	 * Does what the pilot went to Google for, with what came back: signs in
	 * (or goes on to the handle step), links the account, or deletes it.
	 */
	async finishGoogle(returned: GoogleReturn): Promise<void> {
		this.googleRefusal = undefined;
		if ('refusal' in returned) {
			this.googleRefusal = { intent: returned.intent, words: returned.refusal };
			return;
		}
		const { intent, idToken } = returned;
		// A sync that started with the page goes first: the token waits for it.
		for (let waited = 0; this.working && waited < 600; waited += 1) {
			await new Promise((done) => setTimeout(done, 100));
		}
		let refusal: string | undefined;
		if (intent === 'signIn') {
			const entered = await this.continueWithGoogle(idToken);
			this.googleStep = entered.signUp;
			refusal = entered.refusal;
		} else if (intent === 'link') {
			refusal = await this.linkGoogle(idToken);
		} else {
			refusal = await this.deleteAccountWithGoogle(idToken);
		}
		if (refusal) this.googleRefusal = { intent, words: refusal };
	}

	/** The handle step after a first sign-in with Google: creates the account and signs in. */
	async createGoogleAccount(wanted: NewGoogleAccount): Promise<string | undefined> {
		const refusal = await this.#enter(() => this.#core.createGoogleAccount(wanted));
		if (!refusal) this.googleStep = undefined;
		return refusal;
	}

	/** Links the account to the Google account of an ID token. Returns why not, in words. */
	linkGoogle(idToken: string): Promise<string | undefined> {
		return this.#changeAccount(() => this.#core.linkGoogle(idToken));
	}

	/** Unlinks the account from its Google account. Returns why not, in words. */
	unlinkGoogle(): Promise<string | undefined> {
		return this.#changeAccount(() => this.#core.unlinkGoogle());
	}

	/** Sets the pilot's country, two letters, or clears it. Returns why not, in words. */
	setCountry(country: string | undefined): Promise<string | undefined> {
		return this.#changeAccount(() => this.#core.setCountry(country));
	}

	/** Sets the class of the glider the pilot flies now, or clears it. Returns why not, in words. */
	setGliderClass(gliderClass: GliderClassKind | undefined): Promise<string | undefined> {
		return this.#changeAccount(() => this.#core.setGliderClass(gliderClass));
	}

	/** "New flights are public": whether flights sent from now on are public from the start. */
	setNewFlightsPublic(open: boolean): Promise<string | undefined> {
		return this.#changeAccount(() => this.#core.setNewFlightsPublic(open));
	}

	/** "Show me among pilots near a place": the pilot's own setting. Returns why not, in words. */
	setShownNearPlaces(shown: boolean): Promise<string | undefined> {
		return this.#changeAccount(() => this.#core.setShownNearPlaces(shown));
	}

	/** Sends a picture as the pilot's own, as `picture.ts` makes it ready. Returns why not, in words. */
	setPicture(bytes: Uint8Array, mediaType: string): Promise<string | undefined> {
		return this.#changeAccount(() => this.#core.setPicture(bytes, mediaType));
	}

	/** Removes the pilot's picture. Returns why not, in words. */
	removePicture(): Promise<string | undefined> {
		return this.#changeAccount(() => this.#core.removePicture());
	}

	/** The core's question before every flight is made public or private; `flights` 0 when not known. */
	allFlightsQuestion(flights: number, visibility: FlightVisibility): Promise<string> {
		return this.#core.allFlightsQuestion(flights, visibility);
	}

	/**
	 * "Make all public" or "Make all private", after the pilot answered the
	 * core's question: every flight on the server at once. The answer's line
	 * stands in `allFlights`. Returns why not, in words.
	 */
	async setAllFlights(visibility: FlightVisibility): Promise<string | undefined> {
		if (this.working) return undefined;
		this.working = true;
		try {
			this.allFlights = await this.#core.setAllFlights(visibility);
			return undefined;
		} catch (error) {
			return words(error);
		} finally {
			this.working = false;
		}
	}

	async #changeAccount(ask: () => Promise<AccountView | undefined>): Promise<string | undefined> {
		if (this.working) return undefined;
		this.working = true;
		try {
			this.account = (await ask()) ?? this.account;
			return undefined;
		} catch (error) {
			return words(error);
		} finally {
			this.working = false;
		}
	}

	/**
	 * Enters the account `ask` gives and syncs; one that gives none, as a
	 * first sign-in with Google, enters nothing.
	 */
	async #enter(ask: () => Promise<AccountView | undefined>): Promise<string | undefined> {
		if (this.working) return undefined;
		this.working = true;
		try {
			const entered = await ask();
			if (!entered) return undefined;
			this.account = entered;
			await this.#runSync();
			return undefined;
		} catch (error) {
			return words(error);
		} finally {
			this.working = false;
		}
	}

	/**
	 * What a report about such a thing as `subject` can say: the reasons,
	 * how long the pilot's own words may be, and what happens to a report.
	 * The core's, the same for every subject of a kind, so it is asked for
	 * once for each kind: a flight's and a group's differ.
	 */
	reportForm(subject: ReportSubject): Promise<ReportForm> {
		let form = this.#reportForms.get(subject.kind);
		if (!form) {
			form = this.#core.reportForm(subject);
			this.#reportForms.set(subject.kind, form);
		}
		return form;
	}

	/**
	 * Reports a flight, a pilot's page or a live track to whoever runs the
	 * server, from this account. Answers in a line what became of the
	 * report; throws in words when it could not be sent.
	 */
	async report(subject: ReportSubject, reason: ReportReason, text: string): Promise<string> {
		try {
			return await this.#core.report(subject, reason, text);
		} catch (error) {
			throw new Error(words(error));
		}
	}

	/**
	 * What is done while the session still holds, before signing out: the
	 * app ends this browser's subscription to Web Push here.
	 */
	beforeSignOut: (() => Promise<unknown>) | undefined;

	/** Called once the browser is signed out, or the account deleted. */
	afterLeaving: (() => void) | undefined;

	/** Signs this browser out. The pilot's flights stay on the account. */
	async signOut(): Promise<void> {
		if (this.working) return;
		this.working = true;
		try {
			// A browser that was told of this account's alarms is told no more.
			await this.beforeSignOut?.().catch(() => {});
			await this.#core.signOut();
			this.#forget();
		} catch (error) {
			this.problem = words(error);
		} finally {
			this.working = false;
		}
	}

	/**
	 * Deletes the account on the server, with its flights. Returns why it
	 * did not work, if it did not.
	 */
	async deleteAccount(password: string): Promise<string | undefined> {
		if (this.working) return undefined;
		this.working = true;
		try {
			await this.#core.deleteAccount(password);
			this.#forget();
			return undefined;
		} catch (error) {
			return words(error);
		} finally {
			this.working = false;
		}
	}

	/** Deletes an account that signs in with Google, given a fresh ID token of it. As `deleteAccount`. */
	async deleteAccountWithGoogle(idToken: string): Promise<string | undefined> {
		if (this.working) return undefined;
		this.working = true;
		try {
			await this.#core.deleteAccountWithGoogle(idToken);
			this.#forget();
			return undefined;
		} catch (error) {
			return words(error);
		} finally {
			this.working = false;
		}
	}

	#forget(): void {
		this.account = undefined;
		this.problem = undefined;
		this.allFlights = undefined;
		this.#lastSyncAt = undefined;
		this.afterLeaving?.();
	}

	/**
	 * Brings the pilot's own sites in step with the account, if the pilot
	 * is signed in. Called when the site is opened or comes back to the
	 * front. `force` skips the pause between runs. It is quiet: being
	 * offline is no news.
	 */
	async sync({ force = false } = {}): Promise<void> {
		if (!this.signedIn || this.working) return;
		if (
			!force &&
			this.#lastSyncAt !== undefined &&
			this.#now() - this.#lastSyncAt < PAUSE_BETWEEN_SYNCS_MS
		) {
			return;
		}
		this.working = true;
		try {
			await this.#runSync();
		} finally {
			this.working = false;
		}
	}

	async #runSync(): Promise<void> {
		this.#lastSyncAt = this.#now();
		let keptSites: SyncSummary | undefined;
		try {
			keptSites = await this.#core.syncKeptSites();
		} catch {
			// What was done so far is kept; the next sync goes on.
		}
		await this.refresh();
		await this.#onSynced(keptSites);
	}

	/**
	 * Asks the server what it says of the account now, which the core
	 * remembers: its name, and whether it is a partner of Venti for flying
	 * sites. When the app starts and when the account's page opens. Signed
	 * out, nothing is asked.
	 */
	async ask(): Promise<void> {
		if (!this.account) return;
		try {
			this.account = await this.#core.refreshAccount();
		} catch {
			// The screens keep what they have.
		}
	}

	/** Reads the account as it is stored now: after a sync, or a change in another tab. */
	async refresh(): Promise<void> {
		try {
			this.account = await this.#core.account();
		} catch {
			// The screens keep what they have.
		}
	}

	/**
	 * "Programs with access": the words of the screen and of the form that
	 * gives a program access. The core's, asked for once.
	 */
	programsForm(): Promise<TokenForm> {
		this.#programsForm ??= this.#core.tokenForm();
		// One that failed is asked for again.
		this.#programsForm.catch(() => (this.#programsForm = undefined));
		return this.#programsForm;
	}

	/**
	 * The programs that have access to the account, the newest first. Each
	 * of the four below throws an error whose message is in words for the
	 * pilot: the core's refusal, or that Venti could not be reached.
	 */
	programs(): Promise<ProgramView[]> {
		return inWords(this.#core.programs());
	}

	/** A program's name as it will be kept, or the core's words on why it cannot be one. */
	checkProgramName(name: string): Promise<string> {
		return inWords(this.#core.tokenNameCheck(name));
	}

	/**
	 * Gives a program access. The answer holds the token's secret, this
	 * once; nothing here keeps it.
	 */
	giveAccess(wanted: AccessWanted): Promise<TokenCreated> {
		return inWords(this.#core.giveAccess(wanted));
	}

	/** Withdraws a program's access, by its token's identifier. */
	withdrawAccess(id: string): Promise<void> {
		return inWords(this.#core.withdrawAccess(id));
	}
}

/** An answer of the core's, or an error whose message is in words for the pilot. */
async function inWords<T>(asked: Promise<T>): Promise<T> {
	try {
		return await asked;
	} catch (error) {
		throw new Error(words(error), { cause: error });
	}
}

/** Why something about the account did not work, for the pilot. */
function words(error: unknown): string {
	// The core's refusals and a server out of reach are worded already.
	if (isInWords(error)) return (error as Error).message;
	return reasonFor(error);
}
