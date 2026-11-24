import { describe, expect, it } from 'vitest';
import type { ChatMessage, ChatPage } from '#lib/core/index.ts';
import { cursorBefore, merged, newestCursor, postedWords } from './chat';
import { type ChatCore, ChatModel } from './chat.svelte';

function message(cursor: string, text = `message ${cursor}`, removed?: string): ChatMessage {
	return {
		id: `m${cursor}`,
		cursor,
		authorHandle: 'mara',
		authorName: 'Mara Muster',
		isMine: false,
		text: removed ? '' : text,
		pictures: [],
		pictureIds: [],
		cards: [],
		plan: undefined,
		postedAt: '2026-11-18T17:35:00Z',
		removedWords: removed
	};
}

const page = (messages: ChatMessage[], since: string, older?: string, more = false): ChatPage => ({
	messages,
	since,
	older,
	more
});

describe('the messages of a chat', () => {
	it('compares cursors as the numbers they are', () => {
		expect(cursorBefore('9', '10')).toBe(true);
		expect(cursorBefore('10', '9')).toBe(false);
		expect(cursorBefore('12', '13')).toBe(true);
		expect(cursorBefore('13', '13')).toBe(false);
	});

	it('stand the oldest first, whichever order a page brought them in', () => {
		// A page is the newest first.
		const shown = merged([], [message('14'), message('13'), message('9')]);
		expect(shown.map((one) => one.cursor)).toEqual(['9', '13', '14']);
		expect(newestCursor(shown)).toBe('14');
		expect(newestCursor([])).toBeUndefined();
	});

	it('take the page before at the top and what is new at the bottom, each once', () => {
		const first = merged([], [message('14'), message('13')]);
		const older = merged(first, [message('12'), message('11')]);
		const newer = merged(older, [message('15'), message('14')]);
		expect(newer.map((one) => one.cursor)).toEqual(['11', '12', '13', '14', '15']);
	});

	it('keep a removed message in its place, without its words', () => {
		const shown = merged([], [message('14'), message('13')]);
		const after = merged(shown, [message('13', '', 'Removed by an admin')]);
		expect(after.map((one) => [one.cursor, one.text, one.removedWords])).toEqual([
			['13', '', 'Removed by an admin'],
			['14', 'message 14', undefined]
		]);
	});

	it('say a time for today’s and a day with the time before that', () => {
		const now = new Date('2026-11-18T20:00:00');
		const today = new Date('2026-11-18T14:05:00').toISOString();
		const before = new Date('2026-11-16T09:30:00').toISOString();
		expect(postedWords(today, now, 'en-GB')).toBe('14:05');
		expect(postedWords(before, now, 'en-GB')).toBe('16 Nov, 09:30');
		expect(postedWords('not a time', now)).toBe('');
	});
});

/** A core that answers a chat's requests from lists, and notes what it was asked. */
function fakeCore(pages: ChatPage[], since: ChatPage[]) {
	const asked: string[] = [];
	const core: ChatCore = {
		messages: async (_id, before) => {
			asked.push(before ? `before ${before}` : 'newest');
			return pages.shift()!;
		},
		messagesSince: async (_id, from) => {
			asked.push(`since ${from}`);
			return since.shift() ?? page([], from);
		},
		postMessage: async (_id, draft) => ({ ...message('99', draft.text), isMine: true }),
		removeMessage: async (_id, id) => ({ ...message(id.slice(1), '', 'Removed by its author') }),
		markCommunityRead: async (_id, cursor) => {
			asked.push(`read ${cursor}`);
			return { muted: false, unread: 0 };
		},
		sendCommunityPicture: async () => ({ id: 'p', path: '/p', width: 1, height: 1 })
	};
	return { core, asked };
}

describe('a chat that is open', () => {
	it('reads the newest page, marks it read, asks what is new, and the page before when scrolled up', async () => {
		const { core, asked } = fakeCore(
			[page([message('14'), message('13')], 's1', 'o1'), page([message('12')], 's1')],
			[page([message('15')], 's2', undefined, true), page([], 's3')]
		);
		let marked = 0;
		const chat = new ChatModel(core, 'g', () => (marked += 1), 60_000);
		const stop = chat.open();
		await chat.refresh();
		expect(chat.messages.map((one) => one.cursor)).toEqual(['13', '14', '15']);
		expect(chat.hasOlder).toBe(true);
		await chat.loadOlder();
		expect(chat.messages.map((one) => one.cursor)).toEqual(['12', '13', '14', '15']);
		expect(chat.hasOlder).toBe(false);
		stop();
		// While the server said "more", it was asked again at once.
		expect(asked).toEqual(['newest', 'read 14', 'since s1', 'since s2', 'read 15', 'before o1']);
		expect(marked).toBe(2);
	});

	it('shows a posted message at once, and a removed one without its words', async () => {
		const { core } = fakeCore([page([message('14')], 's1')], []);
		const chat = new ChatModel(core, 'g', () => {}, 60_000);
		const stop = chat.open();
		await chat.refresh();
		await chat.post({ text: 'Who flies today?', pictures: [], links: [] });
		expect(chat.messages.at(-1)).toMatchObject({ text: 'Who flies today?', isMine: true });
		await chat.remove(chat.messages[0]);
		expect(chat.messages[0]).toMatchObject({ cursor: '14', removedWords: 'Removed by its author' });
		stop();
	});
});
