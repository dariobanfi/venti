import { describe, expect, it } from 'vitest';
import type { LogbookEntry, LogbookView } from '#lib/core/index.ts';
import { matches, NEWEST_FIRST, sortBy, tableRows } from './table';

function flight(id: string, start: number, more: Partial<LogbookEntry> = {}): LogbookEntry {
	return {
		id,
		startUnixS: start,
		durationS: 3600,
		trackLengthM: 10_000,
		maxAltitudeM: 2000,
		maxClimbMs: 2,
		pilot: undefined,
		glider: undefined,
		outline: [],
		score: undefined,
		takeoffName: undefined,
		...more
	} as LogbookEntry;
}

const view = {
	years: [
		{
			year: 2026,
			flights: [
				flight('c', 30, { takeoffName: 'Wallberg', glider: 'Rush 6', durationS: 7200 }),
				flight('b', 20, { takeoffName: 'Brauneck', trackLengthM: 40_000 })
			]
		},
		{ year: 2025, flights: [flight('a', 10, { takeoffName: 'Tegelberg', glider: 'Rush 6' })] }
	]
} as unknown as LogbookView;

const ids = (rows: LogbookEntry[]) => rows.map((row) => row.id);

describe('the logbook as a table', () => {
	it('lists every year’s flights, the newest first', () => {
		expect(ids(tableRows(view, undefined, '', NEWEST_FIRST))).toEqual(['c', 'b', 'a']);
	});

	it('narrows to a year and to what the pilot searches for', () => {
		expect(ids(tableRows(view, 2026, '', NEWEST_FIRST))).toEqual(['c', 'b']);
		expect(ids(tableRows(view, undefined, 'rush', NEWEST_FIRST))).toEqual(['c', 'a']);
		expect(ids(tableRows(view, undefined, 'rush teg', NEWEST_FIRST))).toEqual(['a']);
		expect(matches(flight('x', 0), '  ')).toBe(true);
	});

	it('sorts by a column, and a second click turns the order round', () => {
		const distance = sortBy(NEWEST_FIRST, 'distance');
		expect(distance).toEqual({ key: 'distance', descending: true });
		expect(ids(tableRows(view, undefined, '', distance))).toEqual(['b', 'c', 'a']);
		expect(ids(tableRows(view, undefined, '', sortBy(distance, 'distance')))[0]).toBe('c');
		const place = sortBy(NEWEST_FIRST, 'place');
		expect(place.descending).toBe(false);
		expect(ids(tableRows(view, undefined, '', place))).toEqual(['b', 'a', 'c']);
		expect(ids(tableRows(view, undefined, '', sortBy(NEWEST_FIRST, 'airtime')))[0]).toBe('c');
	});
});
