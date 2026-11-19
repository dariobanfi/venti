/**
 * The core's files in IndexedDB, the one storage every browser gives a page
 * that holds megabytes, survives a reload and is shared by all tabs of the
 * site.
 *
 * One database with two object stores:
 *
 * - `files`: a file's bytes under its name, exactly the names the core uses
 *   on a phone's disk.
 * - `meta`: the store's revision, a counter that goes up with every change.
 *
 * Every change is one transaction over both, and it first checks that the
 * revision is still the one the tab last saw. Two tabs therefore cannot
 * overwrite each other's work: the second one is told it is out of date,
 * reads the store again and repeats its change.
 */

import { type FileChange, type FileStore, type Snapshot, StaleStoreError } from './store';

const FILES = 'files';
const META = 'meta';
const REVISION = 'revision';

export class IdbStore implements FileStore {
	readonly durable = true;

	#database: IDBDatabase;
	/** Tells the site's other tabs about changes, where the browser can. */
	#channel: BroadcastChannel | undefined;
	#listeners = new Set<() => void>();

	private constructor(database: IDBDatabase, name: string) {
		this.#database = database;
		// A later version of the app may need to change the layout. Let go
		// of the database then, or that tab would wait for this one for ever.
		database.onversionchange = () => database.close();

		if (typeof BroadcastChannel === 'function') {
			this.#channel = new BroadcastChannel(`${name}-changes`);
			this.#channel.onmessage = () => {
				for (const listener of this.#listeners) listener();
			};
		}
	}

	/**
	 * Opens the database, creating it on first use. Rejects when the
	 * browser gives the page no storage, as some do in private windows.
	 */
	static open(name = 'venti', factory: IDBFactory = indexedDB): Promise<IdbStore> {
		return new Promise((resolve, reject) => {
			const request = factory.open(name, 1);
			request.onupgradeneeded = () => {
				request.result.createObjectStore(FILES);
				request.result.createObjectStore(META);
			};
			request.onsuccess = () => resolve(new IdbStore(request.result, name));
			request.onerror = () => reject(request.error);
			request.onblocked = () => reject(new Error('The database is in use by an older page.'));
		});
	}

	async snapshot(): Promise<Snapshot> {
		const transaction = this.#database.transaction([FILES, META], 'readonly');
		const [names, revision] = await Promise.all([
			result(transaction.objectStore(FILES).getAllKeys()),
			result<number | undefined>(transaction.objectStore(META).get(REVISION))
		]);
		return { names: names.map(String), revision: revision ?? 0 };
	}

	read(name: string): Promise<Uint8Array | undefined> {
		const transaction = this.#database.transaction(FILES, 'readonly');
		return result<Uint8Array | undefined>(transaction.objectStore(FILES).get(name));
	}

	async commit(changes: FileChange[], revision: number): Promise<number> {
		const transaction = this.#database.transaction([FILES, META], 'readwrite');
		const finished = completion(transaction);
		const meta = transaction.objectStore(META);

		const stored = (await result<number | undefined>(meta.get(REVISION))) ?? 0;
		if (stored !== revision) {
			transaction.abort();
			await finished.catch(() => {});
			throw new StaleStoreError();
		}

		const files = transaction.objectStore(FILES);
		for (const [name, contents] of changes) {
			if (contents) files.put(contents, name);
			else files.delete(name);
		}
		meta.put(revision + 1, REVISION);
		// A full disk shows here: the transaction aborts and nothing of it is kept.
		await finished;

		this.#channel?.postMessage(revision + 1);
		return revision + 1;
	}

	onChangedElsewhere(listener: () => void): () => void {
		this.#listeners.add(listener);
		return () => this.#listeners.delete(listener);
	}

	close(): void {
		this.#listeners.clear();
		this.#channel?.close();
		this.#database.close();
	}
}

function result<T>(request: IDBRequest<T>): Promise<T> {
	return new Promise((resolve, reject) => {
		request.onsuccess = () => resolve(request.result);
		request.onerror = () => reject(request.error);
	});
}

/** Settles when the transaction has been written to disk or has failed as a whole. */
function completion(transaction: IDBTransaction): Promise<void> {
	return new Promise((resolve, reject) => {
		transaction.oncomplete = () => resolve();
		const failed = () =>
			reject(transaction.error ?? new DOMException('The change was abandoned.', 'AbortError'));
		transaction.onerror = failed;
		transaction.onabort = failed;
	});
}
