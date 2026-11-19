/**
 * Uses the app the way a pilot would, from an empty logbook to a flight on
 * the map and back to empty, and keeps a screenshot of every screen.
 *
 * The same journey runs at a phone's width and at a desktop's, in light and
 * in dark appearance: the four projects of `playwright.config.ts`. The
 * screenshots land in `screenshots/<project>/`. Look at them after every
 * change to a screen. The iPhone app's `VentiUITests.swift` is this test's
 * twin.
 */

import { readFileSync } from 'node:fs';
import {
	addSampleFlight,
	expect,
	flightDetails,
	metric,
	panel,
	SAMPLE_FLIGHT,
	test
} from './helpers';

test('a pilot imports the sample flight, inspects it and deletes it', async ({
	page,
	wide,
	mapDrawn,
	keep,
	problems
}) => {
	const { totals, moment } = panel(page);
	const details = flightDetails(page);
	const logbookTotals = page.getByLabel('All flights');
	const more = page.getByRole('button', { name: 'More' });

	// The app opens on the map.
	await page.goto('/');
	await expect(page).toHaveTitle('Map · Venti');
	await expect(page.getByRole('button', { name: 'Show my position' })).toBeVisible();
	await mapDrawn();
	await keep('01-map');

	// A new logbook says what to do next.
	await page.getByRole('link', { name: 'Logbook', exact: true }).click();
	await expect(page.getByRole('heading', { name: 'No flights yet' })).toBeVisible();
	await expect(page.getByRole('button', { name: 'Import flights' }).first()).toBeVisible();
	await keep('02-logbook-empty');

	// A single imported flight opens straight away.
	await page.getByRole('button', { name: 'Add a sample flight' }).click();
	await expect(page.getByRole('heading', { name: 'Sat, 4 Jul 2026' })).toBeVisible();
	await expect(page.getByText('12:30 – 12:59 · Sample Wing')).toBeVisible();
	await expect(metric(totals, 'Airtime')).toHaveText('0:29 h');
	await expect(metric(totals, 'Track')).toHaveText('18.4 km');
	await expect(metric(totals, 'Highest')).toHaveText('2,117 m');
	await expect(metric(totals, 'Best climb')).toHaveText('+2.4 m/s');
	await expect(page.getByRole('img', { name: /Track colours/ })).toBeVisible();
	await expect(page.getByRole('region', { name: 'Map of the flight' })).toBeVisible();
	await mapDrawn();
	await keep('03-flight');

	// Dragging along the altitude chart marks a moment: the numbers change
	// to it, and it stays when the finger lifts.
	const chart = page.getByRole('slider', { name: 'Altitude over time' });
	const plot = (await chart.boundingBox())!;
	const along = (share: number) => [plot.x + plot.width * share, plot.y + plot.height / 2] as const;
	await page.mouse.move(...along(0.15));
	await page.mouse.down();
	await expect(metric(moment, 'In the air')).toHaveText('0:04 h');
	await page.mouse.move(...along(0.3), { steps: 6 });
	await expect(metric(moment, 'Time')).toHaveText('12:38');
	await expect(metric(moment, 'In the air')).toHaveText('0:09 h');
	await page.mouse.up();
	// Moving on without pressing changes nothing: the mark is where it was left.
	await page.mouse.move(...along(0.8));
	await expect(page.getByRole('button', { name: 'Show flight totals' })).toBeVisible();
	await expect(metric(moment, 'Time')).toHaveText('12:38');
	await expect(metric(moment, 'Altitude')).toHaveText(/^\d,\d{3}\sm$/);
	await expect(metric(moment, 'Climb')).toHaveText(/^[+-]?\d\.\d\sm\/s$/);
	await expect(metric(totals, 'Airtime')).toBeHidden();
	await expect(chart).toHaveAttribute('aria-valuetext', /^12:38, [\d,]+ metres, /);
	await keep('04-flight-moment');

	// The button in the panel's corner returns to the totals.
	await page.getByRole('button', { name: 'Show flight totals' }).click();
	await expect(metric(totals, 'Best climb')).toHaveText('+2.4 m/s');
	await expect(page.getByRole('button', { name: 'Show flight totals' })).toBeHidden();

	// Playing moves the mark through the flight by itself, until it is paused.
	await page.getByRole('button', { name: 'Play the flight' }).click();
	await expect(page.getByRole('button', { name: 'Pause' })).toBeVisible();
	await expect(metric(moment, 'In the air')).toHaveText(/^0:0[1-9]\sh$/);
	await page.getByRole('button', { name: 'Pause' }).click();
	await expect(page.getByRole('button', { name: 'Play the flight' })).toBeVisible();
	const paused = (await metric(moment, 'Altitude').textContent())!;
	await page.waitForTimeout(400);
	await expect(metric(moment, 'Altitude')).toHaveText(paused);
	await page.getByRole('button', { name: 'Show flight totals' }).click();
	await expect(metric(totals, 'Airtime')).toHaveText('0:29 h');

	// Everything else known about the flight: a sheet on a phone, on screen
	// all the time where there is room.
	const sheet = page.getByRole('dialog', { name: 'Flight details' });
	if (!wide) {
		await page.getByRole('button', { name: 'Flight details' }).click();
		await expect(sheet).toBeVisible();
	}
	await expect(metric(details, 'Landing')).toHaveText('12:59');
	await expect(metric(details, 'Lowest')).toHaveText('786 m');
	await expect(metric(details, 'Strongest sink')).toHaveText('-1.9 m/s');
	await expect(metric(details, 'Pilot')).toHaveText('Sample Pilot');
	await expect(metric(details, 'Recorder')).toHaveText('XVE 000');
	await expect(metric(details, 'Positions')).toHaveText('1,745');
	await expect(details.getByText('Climbing faster than 2 m/s')).toBeVisible();
	// What the core worked out about the flight. Which numbers come out is
	// the core's business and tested there; here they have to be on screen.
	await expect(metric(details, 'Scored as')).toHaveText(
		/^(Free flight|Free triangle|FAI triangle)/
	);
	await expect(metric(details, 'Distance')).toHaveText(/^\d+\.\d km$/);
	await expect(metric(details, 'Points')).toHaveText(/^\d+\.\d\d$/);
	await expect(
		details.getByText(/World XContest's published rules, at\s+[\d.]+ points per kilometre/)
	).toBeVisible();
	await expect(metric(details, 'In thermals')).toHaveText(/^0:\d\d h, \d+%$/);
	await expect(metric(details, 'On glides')).toHaveText(/^0:\d\d h, \d+%$/);
	await expect(metric(details, 'Gained in thermals')).toHaveText(/^[\d,]+ m$/);
	const thermals = details.getByRole('list', { name: 'Thermals' }).getByRole('button');
	await expect(thermals.first()).toContainText(/\d:\d\d min/);
	await expect(thermals.first()).toContainText(/\+\d+ m/);
	const glides = details.getByRole('list', { name: 'Glides' }).getByRole('button');
	await expect(glides.first()).toContainText(/\d+ km\/h/);
	await keep('05-flight-details');

	// Choosing a thermal marks the moment it began, on the map and the profile.
	await thermals.first().click();
	if (!wide) await expect(sheet).toBeHidden();
	await expect(page.getByRole('button', { name: 'Show flight totals' })).toBeVisible();
	await expect(metric(moment, 'Time')).toHaveText(/^12:\d\d$/);
	await page.getByRole('button', { name: 'Show flight totals' }).click();

	// The sheet also closes with its own button.
	if (!wide) {
		await page.getByRole('button', { name: 'Flight details' }).click();
		await page.getByRole('button', { name: 'Done' }).click();
		await expect(sheet).toBeHidden();
	}

	// The web can tilt the map into the terrain.
	const terrain = page.getByRole('button', { name: 'Terrain in 3D' });
	await expect(terrain).toHaveAttribute('aria-pressed', 'false');
	await terrain.click();
	await expect(terrain).toHaveAttribute('aria-pressed', 'true');
	await mapDrawn();
	await page.waitForTimeout(1200);
	await keep('06-flight-terrain-3d');
	await terrain.click();
	await expect(terrain).toHaveAttribute('aria-pressed', 'false');

	// The flight's file comes back out as it went in, under a recorder's
	// kind of name.
	await more.click();
	await expect(page.getByRole('menu', { name: 'More' })).toBeVisible();
	await keep('07-flight-menu');
	const download = page.waitForEvent('download');
	await page.getByRole('menuitem', { name: 'Download flight file' }).click();
	const file = await download;
	expect(file.suggestedFilename()).toBe('2026-07-04-XVE-000-01.igc');
	expect(readFileSync(await file.path(), 'utf8')).toBe(readFileSync(SAMPLE_FLIGHT, 'utf8'));
	await expect(page.getByRole('menu', { name: 'More' })).toBeHidden();

	// The flight is stored in the browser: a reload of its page finds it.
	await page.reload();
	await expect(page.getByRole('heading', { name: 'Sat, 4 Jul 2026' })).toBeVisible();
	await expect(metric(totals, 'Airtime')).toHaveText('0:29 h');

	// Back to the logbook, where the flight is now listed.
	await page.getByRole('link', { name: 'Logbook', exact: true }).first().click();
	const row = page.getByRole('link', { name: /Sat, 4 Jul/ });
	await expect(row).toBeVisible();
	await expect(row).toContainText('12:30 · Sample Wing');
	await expect(row).toContainText('0:29 h');
	await expect(row).toContainText('12.0 km');
	await expect(metric(logbookTotals, 'Flight')).toHaveText('1');
	await expect(metric(logbookTotals, 'Airtime')).toHaveText('0:29 h');
	await expect(metric(logbookTotals, 'Distance')).toHaveText('12 km');
	await expect(page.getByRole('heading', { name: '2026' })).toBeVisible();
	await expect(page.getByText('1 flight · 0:29 h')).toBeVisible();
	await page.reload();
	await expect(row).toBeVisible();
	await keep('08-logbook');

	// The totals lead to the statistics: a year by month, the gliders, the bests.
	await page.getByRole('link', { name: 'Statistics' }).click();
	await expect(page).toHaveTitle('Statistics · Venti');
	await expect(page.getByRole('heading', { level: 1, name: 'Statistics' })).toBeVisible();
	await expect(page.getByRole('radio', { name: 'Airtime' })).toBeChecked();
	await expect(page.getByRole('table', { name: 'Airtime by month, in hours' })).toContainText(
		'July 2026'
	);
	await expect(metric(page, 'Sample Wing')).toHaveText('1 flight · 0:29 h');
	const longest = page.getByRole('link', { name: /Longest flight/ });
	await expect(longest).toContainText('Sat, 4 Jul 2026');
	await expect(longest).toContainText('0:29 h');
	await expect(page.getByRole('link', { name: /Highest/ })).toContainText('2,117 m');
	await keep('09-statistics');
	await page.getByText('Distance', { exact: true }).click();
	await expect(page.getByRole('radio', { name: 'Distance' })).toBeChecked();
	await expect(page.getByText('In kilometres.')).toBeVisible();
	// Each best leads to its flight.
	await longest.click();
	await expect(page.getByRole('heading', { name: 'Sat, 4 Jul 2026' })).toBeVisible();
	await page.getByRole('link', { name: 'Logbook', exact: true }).first().click();

	// The map tab now shows the flight too.
	await page.getByRole('link', { name: 'Map', exact: true }).click();
	await mapDrawn();
	await keep('10-map-with-flight');

	// Deleting asks first, and "Cancel" keeps the flight.
	await page.getByRole('link', { name: 'Logbook', exact: true }).click();
	await row.click();
	await expect(page.getByRole('heading', { name: 'Sat, 4 Jul 2026' })).toBeVisible();
	await more.click();
	await page.getByRole('menuitem', { name: 'Delete flight' }).click();
	const confirmation = page.getByRole('alertdialog', { name: 'Delete this flight?' });
	await expect(confirmation).toBeVisible();
	await expect(confirmation).toContainText('This cannot be undone.');
	await expect(confirmation.getByRole('button', { name: 'Cancel' })).toBeFocused();
	await mapDrawn();
	await keep('11-delete');
	await confirmation.getByRole('button', { name: 'Cancel' }).click();
	await expect(confirmation).toBeHidden();
	await expect(page.getByRole('heading', { name: 'Sat, 4 Jul 2026' })).toBeVisible();

	await more.click();
	await page.getByRole('menuitem', { name: 'Delete flight' }).click();
	await confirmation.getByRole('button', { name: 'Delete flight' }).click();
	await expect(page.getByRole('heading', { name: 'No flights yet' })).toBeVisible();

	// And it stays deleted.
	await page.reload();
	await expect(page.getByRole('heading', { name: 'No flights yet' })).toBeVisible();

	expect(problems).toEqual([]);
});

test('a flight that is not in the logbook says so', async ({ page, keep }) => {
	await page.goto('/logbook/flight/?id=00000000000000000000000000000000');

	await expect(
		page.getByRole('heading', { name: 'This flight could not be opened' })
	).toBeVisible();
	await expect(page.getByText('This flight is no longer in the logbook.')).toBeVisible();
	await keep('12-flight-not-found');

	await page.getByRole('link', { name: 'Back to the logbook' }).click();
	await expect(page.getByRole('heading', { name: 'No flights yet' })).toBeVisible();
});

test('the flight screen can be used from the keyboard', async ({ page, wide }) => {
	const { totals, moment } = panel(page);
	await addSampleFlight(page);

	// The chart is a slider over the flight: arrow keys move through it.
	const chart = page.getByRole('slider', { name: 'Altitude over time' });
	await chart.focus();
	await expect(chart).toHaveAttribute('aria-valuetext', /Nothing marked/);
	await page.keyboard.press('ArrowRight');
	await expect(metric(moment, 'Time')).toHaveText('12:30');
	await page.keyboard.press('PageUp');
	await page.keyboard.press('ArrowRight');
	await expect(metric(moment, 'In the air')).toHaveText('0:03 h');
	await page.keyboard.press('End');
	await expect(metric(moment, 'Time')).toHaveText('12:59');
	await page.keyboard.press('Home');
	await expect(metric(moment, 'Time')).toHaveText('12:30');

	// Escape lets go of the moment.
	await page.keyboard.press('Escape');
	await expect(metric(totals, 'Airtime')).toHaveText('0:29 h');

	// Dialogs close on Escape and give the focus back.
	if (!wide) {
		const button = page.getByRole('button', { name: 'Flight details' });
		await button.focus();
		await page.keyboard.press('Enter');
		await expect(page.getByRole('dialog', { name: 'Flight details' })).toBeVisible();
		await page.keyboard.press('Escape');
		await expect(page.getByRole('dialog', { name: 'Flight details' })).toBeHidden();
		await expect(button).toBeFocused();
	}

	// The menu opens on its first entry, the arrow keys move through it, and
	// Escape closes it and returns to its button.
	const more = page.getByRole('button', { name: 'More' });
	await more.focus();
	await page.keyboard.press('Enter');
	await expect(page.getByRole('menuitem', { name: 'Download flight file' })).toBeFocused();
	await page.keyboard.press('ArrowDown');
	await expect(page.getByRole('menuitem', { name: 'Delete flight' })).toBeFocused();
	await page.keyboard.press('Escape');
	await expect(page.getByRole('menu', { name: 'More' })).toBeHidden();
	await expect(more).toBeFocused();

	// Deleting from the keyboard: Enter on the safe answer, which has the
	// focus, keeps the flight.
	await page.keyboard.press('ArrowUp');
	await expect(page.getByRole('menuitem', { name: 'Delete flight' })).toBeFocused();
	await page.keyboard.press('Enter');
	const confirmation = page.getByRole('alertdialog', { name: 'Delete this flight?' });
	await expect(confirmation).toBeVisible();
	await page.keyboard.press('Enter');
	await expect(confirmation).toBeHidden();
	await expect(page.getByRole('heading', { name: 'Sat, 4 Jul 2026' })).toBeVisible();
});
