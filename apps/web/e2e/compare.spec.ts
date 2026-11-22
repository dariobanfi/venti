/**
 * Two flights side by side, on the web's flight screen: another pilot's
 * public flight from the development mode, and the pilot's own sample
 * flight. The development mode has no two flights in the air together, so
 * these are laid out each from its first moment.
 */

import type { Page } from '@playwright/test';
import { addSampleFlight, expect, flightDetails, rowsOf, test } from './helpers';

/** Opens Mara Muster's flight of 16 August 2026, from that day's list. */
async function openMarasFlight(page: Page) {
	await page.goto('/logbook/everyone/?order=best&from=2026-08-16&to=2026-08-16');
	await rowsOf(page, 'Everyone’s flights')
		.filter({ hasText: 'Mara Muster' })
		.getByRole('link')
		// Not the take-off's, whose place is written in degrees.
		.filter({ hasNotText: '°' })
		.last()
		.click();
	await expect(page).toHaveURL(/\/logbook\/everyone\/flight\/\?id=/);
	await expect(page.getByRole('slider', { name: 'Altitude over time' })).toBeVisible();
}

test('another pilot’s flight of the same day is laid beside one, and compared in numbers', async ({
	page,
	keep,
	wide,
	problems
}) => {
	await openMarasFlight(page);
	await page.getByRole('button', { name: 'More' }).click();
	await page.getByRole('menuitem', { name: 'Lay another flight beside it' }).click();

	const chooser = page.getByRole('dialog', { name: 'Lay a flight beside it' });
	await expect(chooser.getByRole('heading', { name: 'Flown here that day' })).toBeVisible();
	await expect(chooser).toContainText('Sign in to lay your own flights beside it.');
	const jonas = chooser.getByRole('button', { name: /Jonas Beispiel/ });
	await expect(jonas).toHaveCount(1);
	await keep('93-compare-choose');
	await jonas.click();

	// The other flight's line names whose it is and how the two lie.
	await expect(page).toHaveURL(/beside=public%3A/);
	const line = page
		.getByRole('button', { name: /Jonas Beispiel/ })
		.filter({ hasText: 'Side by side' });
	await expect(line).toContainText('Each is laid out from its first moment.');
	await keep('94-compare-flight');

	// A moment of the flight: where the other glider was then.
	const chart = page.getByRole('slider', { name: 'Altitude over time' });
	await chart.focus();
	await page.keyboard.press('Home');
	await expect(line).toContainText(/m away|Not in the air then/);

	await line.click();
	const sheet = page.getByRole('dialog', { name: 'Side by side' });
	await expect(sheet.getByRole('columnheader', { name: 'This flight' })).toBeVisible();
	await expect(sheet.getByRole('columnheader', { name: 'Jonas Beispiel' })).toBeVisible();
	await expect(sheet.getByRole('rowheader', { name: 'Airtime' })).toBeVisible();
	await expect(sheet.getByRole('rowheader', { name: 'Scored distance' })).toBeVisible();
	await keep('95-compare-numbers');

	// The address holds the comparison: opened again, it is there.
	await sheet.getByRole('button', { name: 'Done' }).click();
	await page.reload();
	await expect(line).toBeVisible();

	await line.click();
	await sheet.getByRole('button', { name: 'Take the other flight away' }).click();
	await expect(page).not.toHaveURL(/beside=/);
	await expect(line).toHaveCount(0);
	if (!wide) await page.getByRole('button', { name: 'Flight details' }).click();
	await expect(flightDetails(page)).toBeVisible();
	expect(problems).toEqual([]);
});

test('the pilot’s own flight is laid beside another pilot’s', async ({ page }) => {
	await addSampleFlight(page);
	await openMarasFlight(page);
	await page.getByRole('button', { name: 'More' }).click();
	await page.getByRole('menuitem', { name: 'Lay another flight beside it' }).click();

	const chooser = page.getByRole('dialog', { name: 'Lay a flight beside it' });
	await chooser.getByRole('button', { name: /Sat, 4 Jul 2026/ }).click();
	await expect(page).toHaveURL(/beside=own%3A/);
	const line = page
		.getByRole('button', { name: /Your flight of Sat, 4 Jul 2026/ })
		.filter({ hasText: 'Side by side' });
	await expect(line).toContainText('apart. Each is laid out from its first moment.');
});
