import { describe, expect, it } from 'vitest';
import { emptyReport, flightToShow, importNotice, type ImportReport } from './import-report';

function report(parts: Partial<ImportReport>): ImportReport {
	return { ...emptyReport(), ...parts };
}

const notFlight = 'This is not a flight file: it has no date. Flight files end in .igc.';

describe('what the pilot is told after an import', () => {
	it('is nothing when every file became a new flight', () => {
		expect(importNotice(report({ added: ['a', 'b'] }))).toBeUndefined();
		expect(importNotice(emptyReport())).toBeUndefined();
	});

	it('names a file that is not a flight and gives the reason', () => {
		expect(importNotice(report({ failures: [{ file: 'notes.txt', reason: notFlight }] }))).toEqual({
			title: '1 file could not be imported',
			lines: [`notes.txt: ${notFlight}`]
		});
	});

	it('still names the failures when other files were imported', () => {
		const notice = importNotice(
			report({ added: ['a'], failures: [{ file: 'notes.txt', reason: notFlight }] })
		);
		expect(notice?.title).toBe('1 file could not be imported');
	});

	it('names four failures and counts the rest', () => {
		const failures = [1, 2, 3, 4, 5, 6].map((number) => ({
			file: `${number}.txt`,
			reason: 'No.'
		}));

		expect(importNotice(report({ failures }))).toEqual({
			title: '6 files could not be imported',
			lines: ['1.txt: No.', '2.txt: No.', '3.txt: No.', '4.txt: No.', 'and 2 more.']
		});
	});

	it('says so when the flights were all there already', () => {
		expect(importNotice(report({ alreadyThere: ['a'] }))).toEqual({
			title: 'Already in your logbook',
			lines: ['This flight was imported before. Nothing was added.']
		});
		expect(importNotice(report({ alreadyThere: ['a', 'b'] }))?.lines).toEqual([
			'These flights were imported before. Nothing was added.'
		]);
	});

	it('stays quiet about known flights when a new one came with them', () => {
		expect(importNotice(report({ added: ['a'], alreadyThere: ['b'] }))).toBeUndefined();
	});
});

describe('the flight opened straight after an import', () => {
	it('is the new one when there is exactly one and nothing failed', () => {
		expect(flightToShow(report({ added: ['a'] }))).toBe('a');
		expect(flightToShow(report({ added: ['a'], alreadyThere: ['b'] }))).toBe('a');
	});

	it('is none when several came, none came or something failed', () => {
		expect(flightToShow(report({ added: ['a', 'b'] }))).toBeUndefined();
		expect(flightToShow(report({ alreadyThere: ['a'] }))).toBeUndefined();
		expect(
			flightToShow(report({ added: ['a'], failures: [{ file: 'x', reason: 'No.' }] }))
		).toBeUndefined();
	});
});
