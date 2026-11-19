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
	type AccountView,
	type CoreClient,
	type FlightSharing,
	type FlightVisibility,
	isInWords,
	type NewAccount,
	type Registration
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
	/** Called after a sync, which may have changed the logbook. */
	#onSynced: () => Promise<void>;
	#lastSyncAt: number | undefined;

	private constructor(
		core: CoreClient,
		account: AccountView | undefined,
		onSynced: () => Promise<void>,
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
		onSynced: () => Promise<void> = async () => {},
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
		try {
			const outcome = await this.#core.sync();
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
		await this.#onSynced();
	}

	/** Reads the account as it is stored now: after a sync, or a change in another tab. */
	async refresh(): Promise<void> {
		try {
			this.account = await this.#core.account();
		} catch {
			// The screens keep what they have.
		}
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

/** Why something about the account did not work, for the pilot. */
function words(error: unknown): string {
	// The core's refusals and a server out of reach are worded already.
	if (isInWords(error)) return (error as Error).message;
	return reasonFor(error);
}
