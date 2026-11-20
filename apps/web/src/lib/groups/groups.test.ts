import { afterEach, describe, expect, it } from 'vitest';
import type { PlanBoard } from '#lib/core/index.ts';
import { UNREACHABLE } from '#lib/core/protocol.ts';
import { createFormat } from '#lib/design/format.ts';
import { type BoardState, BoardWatch, words } from './board';
import { dayAfter, dayOf, dayTitle } from './days';
import { memberName, question } from './words';

const british = createFormat('en-GB', 'Europe/Berlin');
const american = createFormat('en-US', 'America/Denver');

describe('the pilot’s own today', () => {
	const zone = process.env.TZ;
	afterEach(() => {
		if (zone === undefined) delete process.env.TZ;
		else process.env.TZ = zone;
	});

	it('is the date of the clock where the pilot is, not the date in UTC', () => {
		// Half past midnight in Munich on 21 November: UTC still has the 20th.
		const afterMidnight = new Date('2026-11-20T23:30:00Z');
		process.env.TZ = 'Europe/Berlin';
		expect(dayOf(afterMidnight)).toBe('2026-11-21');
		process.env.TZ = 'UTC';
		expect(dayOf(afterMidnight)).toBe('2026-11-20');

		// An evening in California, when UTC has the next day already.
		const evening = new Date('2026-11-21T05:00:00Z');
		process.env.TZ = 'America/Los_Angeles';
		expect(dayOf(evening)).toBe('2026-11-20');
		process.env.TZ = 'UTC';
		expect(dayOf(evening)).toBe('2026-11-21');
	});

	it('is written as the server writes a day', () => {
		process.env.TZ = 'Europe/Berlin';
		expect(dayOf(new Date(2027, 0, 5, 9, 30))).toBe('2027-01-05');
	});
});

describe('what a day is called on the board', () => {
	it('is “Today” and “Tomorrow” by the pilot’s own today, then the weekday and the date', () => {
		expect(dayTitle('2026-11-20', '2026-11-20', british)).toBe('Today');
		expect(dayTitle('2026-11-21', '2026-11-20', british)).toBe('Tomorrow');
		expect(dayTitle('2026-11-22', '2026-11-20', british)).toBe('Sunday, 22 Nov');
		expect(dayTitle('2027-01-02', '2026-11-20', british)).toBe('Saturday, 2 Jan');
		// A plan of yesterday is called by its day, not counted from today.
		expect(dayTitle('2026-11-19', '2026-11-20', british)).toBe('Thursday, 19 Nov');
	});

	it('follows the calendar over the end of a month and of a year', () => {
		expect(dayTitle('2026-12-01', '2026-11-30', british)).toBe('Tomorrow');
		expect(dayTitle('2027-01-01', '2026-12-31', british)).toBe('Tomorrow');
		expect(dayAfter('2028-02-28')).toBe('2028-02-29');
		expect(dayAfter('2027-02-28')).toBe('2027-03-01');
	});

	it('is the same day whatever the reader’s time zone: a plan’s day is a date, not a moment', () => {
		// Denver is eight hours behind Berlin; the date does not move.
		expect(dayTitle('2026-11-22', '2026-11-20', american)).toBe('Sunday, Nov 22');
	});

	it('is the text as it is when that is no day', () => {
		expect(dayTitle('soon', '2026-11-20', british)).toBe('soon');
		expect(dayTitle('2026-11-31', '2026-11-20', british)).toBe('2026-11-31');
		expect(dayAfter('21.11.2026')).toBeUndefined();
	});
});

const BOARD: PlanBoard = { groups: [], days: [], isCutShort: false };

/** A board that is asked for by hand: each question waits until it is answered. */
function watched(
	pilot: () => string | undefined = () => 'demo',
	now = () => new Date(2026, 10, 20, 8, 0)
) {
	const asked: {
		fromDay: string;
		answer: (board: PlanBoard) => void;
		fail: (error: unknown) => void;
	}[] = [];
	const told: BoardState[] = [];
	const watch = new BoardWatch(
		(fromDay) =>
			new Promise<PlanBoard>((answer, fail) => {
				asked.push({ fromDay, answer, fail });
			}),
		pilot,
		(state) => told.push(state),
		now
	);
	return { watch, asked, told };
}

describe('the board of flying plans', () => {
	it('is asked for from the pilot’s own today on', async () => {
		// Twenty to one at night on the 21st, where the pilot is.
		const { watch, asked } = watched(undefined, () => new Date(2026, 10, 21, 0, 40));

		const loading = watch.load();
		expect(asked.map((question) => question.fromDay)).toEqual(['2026-11-21']);
		expect(watch.state).toMatchObject({ board: undefined, loading: true });
		asked[0].answer(BOARD);
		await loading;

		expect(watch.state).toEqual({
			board: BOARD,
			loading: false,
			problem: undefined,
			today: '2026-11-21'
		});
	});

	it('is not asked for while signed out, and a board from before is put away', async () => {
		let pilot: string | undefined = 'demo';
		const { watch, asked } = watched(() => pilot);
		const loading = watch.load();
		asked[0].answer(BOARD);
		await loading;

		pilot = undefined;
		await watch.load();

		expect(asked).toHaveLength(1);
		expect(watch.state).toMatchObject({ board: undefined, loading: false, problem: undefined });
	});

	it('is one pilot’s: another who signs in is not shown the board from before', async () => {
		let pilot = 'demo';
		const { watch, asked } = watched(() => pilot);
		const first = watch.load();
		asked[0].answer(BOARD);
		await first;

		// Signed out and in again as someone else, in this browser.
		pilot = 'mara';
		const second = watch.load();
		// While the server is asked, the other pilot's groups are not there.
		expect(watch.state).toMatchObject({ board: undefined, loading: true });
		const hers: PlanBoard = { ...BOARD, isCutShort: true };
		asked[1].answer(hers);
		await second;
		expect(watch.state.board).toBe(hers);

		// The same pilot asking again keeps the board while the answer is on its way.
		const third = watch.load();
		expect(watch.state).toMatchObject({ board: hers, loading: true });
		asked[2].answer(hers);
		await third;
	});

	it('shows the newest answer only: a board from before a change does not come back', async () => {
		const { watch, asked } = watched();
		const before = watch.load();
		const after = watch.load();
		const newer: PlanBoard = { ...BOARD, isCutShort: true };

		asked[1].answer(newer);
		await after;
		asked[0].answer(BOARD);
		await before;

		expect(watch.state.board).toBe(newer);
		expect(watch.state.loading).toBe(false);
	});

	it('keeps the board it has when the server cannot be asked again, and says why', async () => {
		const { watch, asked } = watched();
		const first = watch.load();
		asked[0].answer(BOARD);
		await first;

		const again = watch.load();
		const unreachable = new Error('Venti could not be reached.');
		unreachable.name = UNREACHABLE;
		asked[1].fail(unreachable);
		await again;

		expect(watch.state).toMatchObject({
			board: BOARD,
			loading: false,
			problem: 'Venti could not be reached.'
		});

		// The next answer takes the words away again.
		const third = watch.load();
		asked[2].answer(BOARD);
		await third;
		expect(watch.state.problem).toBeUndefined();
	});

	it('says the core’s refusals as they are, and what the browser says with what it was about', () => {
		const refusal = new Error('This device is no longer signed in. Sign in again.');
		refusal.name = 'Server';
		expect(words(refusal)).toBe('This device is no longer signed in. Sign in again.');
		expect(words(new TypeError('Out of memory'))).toBe(
			'The groups and their plans could not be reached. Out of memory'
		);
	});
});

describe('what is asked before something is done to a group', () => {
	const mara = { name: 'Mara Muster', handle: 'mara' };

	it('says what it means, each time', () => {
		expect(question({ about: 'newCode' })).toEqual({
			title: 'Make a new invite code?',
			message: 'The code handed out before stops working. Members stay in the group.',
			confirm: 'Make a new code'
		});
		expect(question({ about: 'delete' }).message).toBe(
			'The group goes for every member, with all its plans.'
		);
		expect(question({ about: 'deletePlan' }).message).toBe(
			'It goes for everyone in the group, with who was in.'
		);
		// Removing a member ends the invite code: the keeper is told before.
		expect(question({ about: 'remove', member: mara })).toMatchObject({
			title: 'Remove Mara Muster?',
			confirm: 'Remove from the group'
		});
		expect(question({ about: 'remove', member: mara }).message).toContain(
			'The invite code stops working too'
		);
		expect(question({ about: 'handOver', member: mara })).toMatchObject({
			title: 'Hand the keeping to Mara Muster?',
			confirm: 'Make keeper'
		});
	});

	it('tells a keeper who leaves what becomes of the group', () => {
		const member = question({ about: 'leave', asKeeper: false });
		const keeper = question({ about: 'leave', asKeeper: true });
		expect(member.message).toBe('You will no longer see its plans, and yours are removed.');
		expect(keeper.message).toContain(member.message);
		expect(keeper.message).toContain('The member who has been in longest keeps the group');
	});

	it('marks the pilot among the members', () => {
		expect(memberName({ name: 'Demo Pilot', isMe: true })).toBe('Demo Pilot (you)');
		expect(memberName({ name: 'Mara Muster', isMe: false })).toBe('Mara Muster');
	});
});
