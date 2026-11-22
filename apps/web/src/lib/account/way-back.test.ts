import { describe, expect, it } from 'vitest';
import { forgetWayBack, GOOD_FOR_MS, keepWayBack, takeWayBack } from './way-back';

/** A tab's `sessionStorage`. */
function tab(): Storage {
	const kept = new Map<string, string>();
	return {
		getItem: (key) => kept.get(key) ?? null,
		setItem: (key, value) => void kept.set(key, value),
		removeItem: (key) => void kept.delete(key),
		clear: () => kept.clear(),
		key: () => null,
		get length() {
			return kept.size;
		}
	};
}

const NOW = Date.UTC(2026, 10, 22, 9);

describe('the way back to a page that sent the pilot to sign in', () => {
	it('is one step after a sign-in on the account’s page itself', () => {
		const storage = tab();
		// The asking page was the fourth entry; the account's page is the fifth.
		keepWayBack(5, storage, () => NOW);
		expect(takeWayBack(5, storage, () => NOW + 30_000)).toBe(-1);
	});

	it('is as many steps more as the history grew by on the way through Google', () => {
		const storage = tab();
		keepWayBack(5, storage, () => NOW);
		// Google's page, and the account's page again.
		expect(takeWayBack(7, storage, () => NOW + 60_000)).toBe(-3);
	});

	it('is taken once', () => {
		const storage = tab();
		keepWayBack(5, storage, () => NOW);
		expect(takeWayBack(5, storage, () => NOW)).toBe(-1);
		expect(takeWayBack(5, storage, () => NOW)).toBeUndefined();
	});

	it('is there for a quarter of an hour, and not after the pilot went elsewhere', () => {
		const storage = tab();
		keepWayBack(5, storage, () => NOW);
		expect(takeWayBack(5, storage, () => NOW + GOOD_FOR_MS + 1)).toBeUndefined();
		keepWayBack(5, storage, () => NOW);
		forgetWayBack(storage);
		expect(takeWayBack(5, storage, () => NOW)).toBeUndefined();
		// Nothing was kept at all.
		expect(takeWayBack(5, tab(), () => NOW)).toBeUndefined();
	});

	it('holds a count and a time, and nothing of the page it leads to', () => {
		const storage = tab();
		keepWayBack(5, storage, () => NOW);
		expect(storage.getItem('venti.sign-in-way-back')).toBe(JSON.stringify({ length: 5, at: NOW }));
	});

	it('never leads forward, and is nothing in a browser without storage', () => {
		const storage = tab();
		keepWayBack(5, storage, () => NOW);
		// A history shorter than when it was kept: not this tab's way.
		expect(takeWayBack(3, storage, () => NOW)).toBeUndefined();
		const none = {
			...tab(),
			getItem: () => {
				throw new Error('no storage');
			},
			setItem: () => {
				throw new Error('no storage');
			}
		};
		keepWayBack(5, none, () => NOW);
		expect(takeWayBack(5, none, () => NOW)).toBeUndefined();
	});
});
