/**
 * The way back to a page that sent the pilot to sign in, for a page that
 * can only be answered signed in and whose address must not be written
 * anywhere: `/connect/`, whose address holds a request's identifier.
 *
 * The address stays where it is, in the tab's history, and only there.
 * What is kept is how long that history was when the pilot arrived at the
 * account's page, and when: once the pilot is signed in, the account's
 * page goes back as many steps as the history has grown by since, plus the
 * one that led to it. That holds for a sign-in on the page itself (one
 * step) and for one through Google's page and back (three or more).
 *
 * Kept in `sessionStorage`, which is this tab's alone and survives the
 * trip to Google. It names no page and no request.
 */

/** Where this tab keeps the way back. */
const KEPT = 'venti.sign-in-way-back';

/** How long a way back is good for: a request for agreement waits a quarter of an hour. */
export const GOOD_FOR_MS = 15 * 60_000;

interface Kept {
	/** How many entries the tab's history had on arriving at the account's page. */
	length: number;
	/** When, in milliseconds. */
	at: number;
}

/**
 * Notes where the account's page stands in the tab's history. Called by
 * the page that sent the pilot there, right after the navigation: the
 * account's page is then the newest entry and the asking page the one
 * before it.
 */
export function keepWayBack(
	entries: number = history.length,
	storage: Storage = sessionStorage,
	now: () => number = Date.now
): void {
	try {
		storage.setItem(KEPT, JSON.stringify({ length: entries, at: now() } satisfies Kept));
	} catch {
		// Not kept: the pilot goes back with the browser's own button.
	}
}

/**
 * How many steps back in the tab's history the page stands that sent the
 * pilot to sign in, as a number for `history.go` (below zero), if one did
 * within the last quarter of an hour. The way back is forgotten with the
 * asking: it is taken once.
 */
export function takeWayBack(
	entries: number = history.length,
	storage: Storage = sessionStorage,
	now: () => number = Date.now
): number | undefined {
	let kept: Kept | undefined;
	try {
		kept = JSON.parse(storage.getItem(KEPT) ?? 'null') ?? undefined;
		storage.removeItem(KEPT);
	} catch {
		return undefined;
	}
	if (!kept || !Number.isInteger(kept.length) || typeof kept.at !== 'number') return undefined;
	const age = now() - kept.at;
	if (age < 0 || age > GOOD_FOR_MS) return undefined;
	// The asking page is the entry before the one the pilot arrived at.
	const steps = kept.length - 1 - entries;
	return steps < 0 ? steps : undefined;
}

/** Forgets the way back: the pilot left the account's page for another of the app's. */
export function forgetWayBack(storage: Storage = sessionStorage): void {
	try {
		storage.removeItem(KEPT);
	} catch {
		// Nothing was kept.
	}
}
