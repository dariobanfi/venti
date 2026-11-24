import { describe, expect, it } from 'vitest';
import type { CoreClient, Unread } from '#lib/core/index.ts';
import { LEFT_AS_IT_IS, UnreadModel } from './unread.svelte';

const FIVE: Unread = {
	notifications: 4,
	alarms: 1,
	total: 5,
	groups: 3,
	groupsBadge: '3',
	badge: '5',
	label: '5 new'
};
const NONE: Unread = {
	notifications: 0,
	alarms: 0,
	total: 0,
	groups: 0,
	groupsBadge: undefined,
	badge: undefined,
	label: undefined
};

/** A core that answers from a list, and notes what it was told was seen. */
function fakeCore(answers: (Unread | undefined | Error)[]) {
	const seen: { notifications?: string; alarms?: string }[] = [];
	const next = async () => {
		const answer = answers.shift();
		if (answer instanceof Error) throw answer;
		return answer;
	};
	const core: Pick<CoreClient, 'unread' | 'unreadSeen'> = {
		unread: next,
		unreadSeen: async (lists) => {
			seen.push(lists);
			return next();
		}
	};
	return { core, seen };
}

describe('the marker of what is new', () => {
	it('shows the core’s badge and the words a screen reader says', async () => {
		const model = new UnreadModel(fakeCore([FIVE]).core);
		await model.read();
		expect(model.badge).toBe('5');
		expect(model.label).toBe('5 new');
		expect(model.notifications).toBe(4);
		expect(model.alarms).toBe(1);
	});

	it('keeps the groups’ unread messages beside the marker, as a badge of their own', async () => {
		const { core } = fakeCore([FIVE, NONE]);
		const model = new UnreadModel(core);
		await model.read();
		expect(model.groupsBadge).toBe('3');
		expect(model.groups).toBe(3);
		await model.read();
		expect(model.groupsBadge).toBeUndefined();
		expect(model.groups).toBe(0);
	});

	it('shows nothing signed out, and keeps what it said when offline', async () => {
		const model = new UnreadModel(fakeCore([FIVE, new Error('offline'), undefined]).core);
		await model.read();
		await model.read();
		expect(model.badge).toBe('5');
		await model.read();
		expect(model.badge).toBeUndefined();
		expect(model.label).toBeUndefined();
	});

	it('moves the mark past what a page showed, and shows the count after it', async () => {
		const { core, seen } = fakeCore([FIVE, NONE]);
		const model = new UnreadModel(core);
		await model.read();
		await model.seen({ notifications: '9', alarms: LEFT_AS_IT_IS });
		expect(seen).toEqual([{ notifications: '9', alarms: '0' }]);
		expect(model.badge).toBeUndefined();
		expect(model.notifications).toBe(0);
	});

	it('reads once at a time, and an answer from before signing out is not shown', async () => {
		let answer: (value: Unread) => void = () => {};
		let asked = 0;
		const core: Pick<CoreClient, 'unread' | 'unreadSeen'> = {
			unread: () => {
				asked += 1;
				return new Promise((resolve) => (answer = resolve));
			},
			unreadSeen: async () => NONE
		};
		const model = new UnreadModel(core);
		const one = model.read();
		const two = model.read();
		expect(asked).toBe(1);
		model.clear();
		answer(FIVE);
		await Promise.all([one, two]);
		expect(model.badge).toBeUndefined();
	});
});
