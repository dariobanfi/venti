import { describe, expect, it } from 'vitest';
import { endOfDayUnixS, firstEndDay } from './programs';

describe('the day a program’s access ends', () => {
	it('is good to the last second of that day in UTC, the calendar the core names it by', () => {
		expect(endOfDayUnixS('2026-12-21')).toBe(Date.UTC(2026, 11, 21, 23, 59, 59) / 1000);
		expect(new Date(endOfDayUnixS('2026-12-21')! * 1000).toISOString()).toBe(
			'2026-12-21T23:59:59.000Z'
		);
	});

	it('is none for an empty field and for a day the calendar does not have', () => {
		expect(endOfDayUnixS('')).toBeUndefined();
		expect(endOfDayUnixS('21.12.2026')).toBeUndefined();
		expect(endOfDayUnixS('2026-11-31')).toBeUndefined();
	});

	it('is offered from today by UTC’s calendar', () => {
		expect(firstEndDay(Date.UTC(2026, 10, 21, 23, 30))).toBe('2026-11-21');
		expect(endOfDayUnixS(firstEndDay(Date.UTC(2026, 10, 21, 23, 30)))! * 1000).toBeGreaterThan(
			Date.UTC(2026, 10, 21, 23, 30)
		);
	});
});
