/**
 * "Send to a league", the button on a flight of the logbook: what XContest and the
 * DHV-XC ask about, the flight's file and XContest's claim page. Nothing is
 * sent from the browser: a flight goes to a league from the pilot's own
 * device, and the web app does not send to the DHV-XC.
 */

import { readFileSync } from 'node:fs';
import { addSampleFlight, expect, SAMPLE_FLIGHT, test } from './helpers';

test('a flight is handed to a league with what the leagues ask about', async ({
	page,
	keep,
	problems
}) => {
	// The sample flight was flown on 4 July 2026: fourteen days on, the
	// leagues still took it for their contests.
	await page.clock.setFixedTime(new Date('2026-07-10T12:00:00Z'));
	await addSampleFlight(page);

	await page.getByRole('button', { name: 'Send to a league' }).click();
	const sheet = page.getByRole('dialog', { name: 'Send to a league' });
	await expect(sheet).toContainText('this one until the end of 18 Jul 2026, by UTC');
	// Venti's own recordings carry no signature, and the sheet says what that means.
	await expect(sheet).toContainText('Not signed.');
	await expect(sheet).toContainText('Venti recorded this flight and does not sign its files yet.');
	await expect(sheet.getByRole('link', { name: 'Open XContest’s claim page' })).toHaveAttribute(
		'href',
		'https://www.xcontest.org/world/en/flight-claim/'
	);
	await expect(sheet).toContainText('The web app does not send a flight to the DHV-XC');
	await keep('89-league');

	// The file goes to the pilot as it was imported.
	const download = page.waitForEvent('download');
	await sheet.getByRole('button', { name: 'Save the flight file' }).click();
	const file = await download;
	expect(readFileSync(await file.path(), 'utf8')).toBe(readFileSync(SAMPLE_FLIGHT, 'utf8'));
	expect(problems).toEqual([]);
});

test('a flight flown more than fourteen days ago is said to be late, by a sign and in words', async ({
	page,
	keep
}) => {
	await page.clock.setFixedTime(new Date('2026-11-20T12:00:00Z'));
	await addSampleFlight(page);

	await page.getByRole('button', { name: 'Send to a league' }).click();
	const sheet = page.getByRole('dialog', { name: 'Send to a league' });
	await expect(sheet).toContainText('Too late for the contests.');
	await expect(sheet).toContainText('ended with 18 Jul 2026');
	await keep('90-league-late');
});
