/**
 * Where the browser keeps the core's files.
 *
 * The core decides what is stored: whole files under short names such as
 * `flights/<id>.igc` (see `crates/core/src/storage.rs`). A store only reads
 * and writes them. It knows nothing about flights.
 *
 * Beside the files a store keeps one thing the core never sees: the session
 * token of the pilot's account. A phone keeps it in its Keychain; a browser
 * has nothing of the kind, and `apps/web/AGENTS.md` says what that means.
 */

/**
 * A file to store, or with no contents a file to remove. This is the form
 * in which the core hands over its changes.
 */
export type FileChange = [name: string, contents: Uint8Array | undefined];

/** What a store holds at one moment. */
export interface Snapshot {
	/** The name of every stored file. */
	names: string[];
	/** How many times the store has been changed. It tells a tab that another tab was there. */
	revision: number;
}

export interface FileStore {
	/** Whether what is stored here is still there after the page is closed. */
	readonly durable: boolean;

	snapshot(): Promise<Snapshot>;

	/** The contents of a file, or nothing if no file has that name. */
	read(name: string): Promise<Uint8Array | undefined>;

	/**
	 * Applies the changes, all of them or none, and returns the store's new
	 * revision.
	 *
	 * `revision` is the revision the changes were worked out against. If
	 * another tab has changed the store since, nothing is written and a
	 * {@link StaleStoreError} is thrown: the caller's picture of the store is
	 * out of date and so, possibly, are its changes.
	 *
	 * `token` is a session token to keep from now on, or `null` to forget the
	 * one kept. It is written with the changes or not at all, so the account
	 * the core remembers and the token for it arrive and leave together.
	 */
	commit(changes: FileChange[], revision: number, token?: string | null): Promise<number>;

	/** The session token of the account this browser is signed in to, if any. */
	token(): Promise<string | undefined>;

	/** Forgets the session token, when the server no longer takes it. */
	forgetToken(): Promise<void>;

	/**
	 * Calls `listener` whenever another tab has changed the store. Returns a
	 * function that stops the calls.
	 */
	onChangedElsewhere(listener: () => void): () => void;

	close(): void;
}

/** The store was changed by another tab after the caller last looked at it. */
export class StaleStoreError extends Error {
	constructor() {
		super('The stored files were changed by another tab.');
		this.name = 'StaleStoreError';
	}
}

/**
 * A store that lasts as long as the page. It stands in when the browser
 * refuses the page its storage, so the app still works for one visit, and
 * it is what the tests use where the kind of store does not matter.
 */
export class MemoryStore implements FileStore {
	readonly durable = false;

	#files = new Map<string, Uint8Array>();
	#revision = 0;
	#token: string | undefined;

	async snapshot(): Promise<Snapshot> {
		return { names: [...this.#files.keys()], revision: this.#revision };
	}

	async read(name: string): Promise<Uint8Array | undefined> {
		return this.#files.get(name);
	}

	async commit(changes: FileChange[], revision: number, token?: string | null): Promise<number> {
		if (revision !== this.#revision) throw new StaleStoreError();
		for (const [name, contents] of changes) {
			if (contents) this.#files.set(name, contents);
			else this.#files.delete(name);
		}
		if (token !== undefined) this.#token = token ?? undefined;
		this.#revision += 1;
		return this.#revision;
	}

	async token(): Promise<string | undefined> {
		return this.#token;
	}

	async forgetToken(): Promise<void> {
		this.#token = undefined;
	}

	onChangedElsewhere(): () => void {
		// Nothing else can reach a store that lives in this page's memory.
		return () => {};
	}

	close(): void {}
}
