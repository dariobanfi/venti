import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { PilotsFound } from '#lib/core/index.ts';
import { NOT_SEARCHED, PilotSearch, type SearchState, TYPING_REST_MS } from './search';

const LENA: PilotsFound = {
	pilots: [
		{
			handle: 'lena',
			name: 'Lena Probe',
			flights: 3,
			words: 'Lena Probe (@lena), 3 public flights'
		}
	],
	words: '1 pilot'
};

/** A search over a server the test plays. */
function searching(reply: (text: string) => Promise<PilotsFound | undefined>) {
	const asked: string[] = [];
	const told: SearchState[] = [];
	const search = new PilotSearch(
		(text) => {
			asked.push(text);
			return reply(text);
		},
		(state) => told.push(state)
	);
	return { search, asked, told, state: () => told.at(-1) ?? NOT_SEARCHED };
}

beforeEach(() => {
	vi.useFakeTimers();
});
afterEach(() => {
	vi.useRealTimers();
});

describe('looking for a pilot by name', () => {
	it('asks once the typing has rested, and only for what was typed last', async () => {
		const { search, asked, state } = searching(async () => LENA);

		search.look('l');
		search.look('le');
		search.look('len');
		expect(asked).toEqual([]);
		expect(state()).toEqual({ text: 'len', waiting: true, found: undefined, problem: undefined });

		await vi.advanceTimersByTimeAsync(TYPING_REST_MS - 1);
		expect(asked).toEqual([]);
		await vi.advanceTimersByTimeAsync(1);
		expect(asked).toEqual(['len']);
		expect(state()).toEqual({ text: 'len', waiting: false, found: LENA, problem: undefined });
	});

	it('waits at least 300 ms, which the server’s budget asks for', () => {
		expect(TYPING_REST_MS).toBeGreaterThanOrEqual(300);
	});

	it('does not ask again for the same text, with blanks around it or not', async () => {
		const { search, asked } = searching(async () => LENA);

		search.look('le');
		await vi.advanceTimersByTimeAsync(TYPING_REST_MS);
		search.look(' le ');
		await vi.advanceTimersByTimeAsync(TYPING_REST_MS);

		expect(asked).toEqual(['le']);
	});

	it('clears at once when the field is emptied, and asks nothing', async () => {
		const { search, asked, state } = searching(async () => LENA);

		search.look('le');
		search.look('');
		await vi.advanceTimersByTimeAsync(TYPING_REST_MS);

		expect(asked).toEqual([]);
		expect(state()).toEqual(NOT_SEARCHED);
	});

	it('drops the answer to a question that is no longer asked', async () => {
		let answer!: (found: PilotsFound) => void;
		const { search, asked, state } = searching((text) =>
			text === 'le'
				? new Promise((resolve) => (answer = resolve))
				: Promise.resolve({ pilots: [], words: 'Nobody.' })
		);

		search.look('le');
		await vi.advanceTimersByTimeAsync(TYPING_REST_MS);
		search.look('lex');
		answer(LENA);
		await vi.advanceTimersByTimeAsync(0);
		expect(state()).toMatchObject({ text: 'lex', waiting: true, found: undefined });

		await vi.advanceTimersByTimeAsync(TYPING_REST_MS);
		expect(asked).toEqual(['le', 'lex']);
		expect(state()).toMatchObject({ text: 'lex', waiting: false, found: { words: 'Nobody.' } });
	});

	it('says in the core’s words why a text is not asked for', async () => {
		const refused = new Error('Type at least two letters of a pilot’s name or handle.');
		refused.name = 'Server';
		const { search, state } = searching(() => Promise.reject(refused));

		search.look('l');
		await vi.advanceTimersByTimeAsync(TYPING_REST_MS);

		expect(state().found).toBeUndefined();
		expect(state().problem).toBe('Type at least two letters of a pilot’s name or handle.');
	});
});
