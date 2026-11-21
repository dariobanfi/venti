/**
 * How you fly: what the pilot's own tracks say about how they fly, from the
 * foot of the statistics. The sample flight has two thermals; the core
 * says something once the logbook holds ten, so five copies of it on other
 * days of July 2026 are enough.
 */

import type { Page } from '@playwright/test';
import { expect, metric, sampleFlightOn, test } from './helpers';

const asUploads = (days: number[]) =>
	days.map((day) => ({
		name: `2026-07-${String(day).padStart(2, '0')}.igc`,
		mimeType: 'application/octet-stream',
		buffer: Buffer.from(sampleFlightOn(day))
	}));

/** Imports the sample flight on these days of July with the file picker. */
async function importDays(page: Page, days: number[]) {
	await page.goto('/logbook/');
	const picker = page.waitForEvent('filechooser');
	await page.getByRole('button', { name: 'Import flights' }).first().click();
	await (await picker).setFiles(asUploads(days));
	// A single flight opens as it comes in; the logbook is one step back.
	if (days.length === 1) {
		await expect(page).toHaveURL(/\/logbook\/flight\//);
		await page.goto('/logbook/');
	}
	// The totals call one flight "Flight".
	await expect(
		metric(page.getByLabel('All flights'), days.length === 1 ? 'Flight' : 'Flights')
	).toHaveText(String(days.length));
}

async function openHowYouFly(page: Page) {
	await page.getByRole('link', { name: 'Statistics' }).first().click();
	await page.getByRole('navigation', { name: 'More about your flying' }).getByRole('link').click();
	await expect(page).toHaveTitle('How you fly · Venti');
	await expect(page.getByRole('heading', { level: 1, name: 'How you fly' })).toBeVisible();
}

test('how you fly has nothing to say about one flight', async ({ page }) => {
	await importDays(page, [4]);
	await openHowYouFly(page);
	await expect(
		page.getByText(
			'Nothing to say yet. The observations begin once the logbook holds ten thermals.'
		)
	).toBeVisible();
	// What the observations are and are not, under them in any case.
	await expect(page.getByText(/not a judgement of you or of what is safe/)).toBeVisible();
});

test('how you fly says what the pilot’s tracks show, each with the flight that shows it most', async ({
	page,
	keep,
	problems
}) => {
	await importDays(page, [1, 2, 3, 5, 6]);
	await openHowYouFly(page);

	// Each observation is a group with the core's title and words.
	const circling = page.getByRole('group', { name: 'Which way you circle' });
	await expect(circling).toContainText(/thermals circled one way/);
	const climbing = page.getByRole('group', { name: 'Climbing' });
	await expect(climbing).toContainText(/m\/s on average over \d+ m gained in thermals/);
	await expect(page.getByRole('group', { name: 'Time circling' })).toContainText(
		/% of your airtime was spent in thermals/
	);
	// Never a word of praise or of what is safe.
	await expect(page.getByRole('main').getByText(/\bshould\b|\bgood\b|\bsafe to\b/i)).toHaveCount(0);
	await keep('115-how-you-fly');

	// The flight that shows it most opens that flight.
	const flight = climbing.getByRole('link', { name: /The flight that shows it most/ });
	await expect(flight).toContainText(/\w{3}, \d+ Jul 2026/);
	const day = (await flight.textContent())!.match(/\w{3}, \d+ Jul 2026/)![0];
	await flight.click();
	await expect(page.getByRole('heading', { name: day })).toBeVisible();
	expect(problems).toEqual([]);
});
