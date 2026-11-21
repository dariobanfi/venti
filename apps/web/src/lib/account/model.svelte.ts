/**
 * The pilot's account on the Venti server, as the screens see it.
 *
 * What to ask the server and what its answers mean is the core's, and the
 * requests are sent by the worker (`core/account-source.ts`). This class
 * keeps what the screens draw, decides when a sync runs, and puts what
 * went wrong into words. It is the browser's counterpart of the iPhone
 * app's `AccountModel.swift`.
 */

import {
	type AccessWanted,
	type AccountView,
	type CoreClient,
	type FlightSharing,
	type FlightVisibility,
	isInWords,
	type NewAccount,
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

	/** Whether a request or a sync is under way. */
	working = $state(false);

	/** What the last sync did, in the core's words: "3 sent, 1 fetched". */
	lastSync = $state.raw<string>();

	/** Why the last sync the pilot asked for stopped, in words. */
	problem = $state.raw<string>();

	#core: CoreClient;
	#now: () => number;
	/**
	 * Called after a sync, which may have changed the logbook, with what it
	 * did to the pilot's own sites where their run came to its end.
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
		return model;
	}

	/** Whether the pilot is signed in and the session still holds. */
	get signedIn(): boolean {
		return this.account !== undefined && !this.account.needsSignIn;
	}

	/**
	 * Signs in, and brings the logbook in step. Returns why it did not
	 * work, in words, or nothing when it did.
	 */
	signIn(handle: string, password: string): Promise<string | undefined> {
		return this.#enter(() => this.#core.signIn(handle, password));
	}

	/** Creates an account and signs in. Returns why it did not work, if it did not. */
	createAccount(wanted: NewAccount): Promise<string | undefined> {
		return this.#enter(() => this.#core.createAccount(wanted));
	}

	async #enter(ask: () => Promise<AccountView>): Promise<string | undefined> {
		if (this.working) return undefined;
		this.working = true;
		try {
			this.account = await ask();
			this.lastSync = undefined;
			await this.#runSync(true);
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

	/** Signs this browser out. The flights in the logbook stay. */
	async signOut(): Promise<void> {
		if (this.working) return;
		this.working = true;
		try {
			await this.#core.signOut();
			this.#forget();
		} catch (error) {
			this.problem = words(error);
		} finally {
			this.working = false;
		}
	}

	/**
	 * Deletes the account on the server, with its flights there. The logbook
	 * in this browser stays. Returns why it did not work, if it did not.
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

	#forget(): void {
		this.account = undefined;
		this.lastSync = undefined;
		this.problem = undefined;
		this.#lastSyncAt = undefined;
	}

	/**
	 * Brings the logbook and the server in step, if the pilot is signed in.
	 * Called when the site is opened or comes back to the front, and after
	 * the logbook changed. `force` skips the pause between runs. A run the
	 * pilot did not ask for is quiet: being offline after a flight is no news.
	 */
	async sync({ force = false, quietly = true } = {}): Promise<void> {
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
			await this.#runSync(!quietly);
		} finally {
			this.working = false;
		}
	}

	async #runSync(reportsProblems: boolean): Promise<void> {
		this.#lastSyncAt = this.#now();
		if (reportsProblems) this.problem = undefined;
		let keptSites: SyncSummary | undefined;
		try {
			const outcome = await this.#core.sync();
			keptSites = outcome.keptSites;
			if (outcome.kind === 'finished') {
				this.lastSync = outcome.said;
				this.problem = undefined;
			} else if (reportsProblems || outcome.needsSignIn) {
				this.problem = outcome.message;
			}
		} catch (error) {
			// What was done so far is kept; the next sync goes on.
			if (reportsProblems) this.problem = words(error);
		}
		await this.refresh();
		await this.#onSynced(keptSites);
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

	/** How a flight is shared, if the server has it. */
	async sharing(flight: string): Promise<FlightSharing | undefined> {
		try {
			return await this.#core.flightSharing(flight);
		} catch {
			return undefined;
		}
	}

	/**
	 * Changes who may see a flight, and says how it is shared now. Throws
	 * an error whose message is in words for the pilot.
	 */
	async share(flight: string, visibility: FlightVisibility): Promise<FlightSharing> {
		if (this.working) throw new Error('Another request is under way. Try again in a moment.');
		this.working = true;
		try {
			return await this.#core.shareFlight(flight, visibility);
		} catch (error) {
			throw new Error(words(error));
		} finally {
			this.working = false;
		}
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
