/**
 * The ranking, from the Venti server in its development mode: six made-up
 * pilots with eleven public flights between them, all flown in 2026 but
 * one of Timo Entwurf's, of July 2025. `playwright.config.ts` starts the
 * server.
 *
 * The page offers the year the browser's clock shows and the three before
 * it. These tests name 2026, which is among them until 2029.
 */

import type { Page } from '@playwright/test';
import { expect, rowsOf, test } from './helpers';

const pilots = (page: Page) => rowsOf(page, /^Ranking of /);
/** How many pilots the development mode ranks, in all years and in 2026: 6 since 2026-11-21. */
const RANKED = 6;
const whose = (page: Page) => page.getByRole('navigation', { name: 'Whose flights' });
/** The flights of a pilot that count: a list under the name on a phone, a column each in a wide table. */
const counted = (page: Page, pilot: string) =>
	page
		.getByRole('list', { name: `The flights of ${pilot} that count` })
		.getByRole('listitem')
		.or(
			pilots(page)
				.filter({ has: page.getByRole('link', { name: pilot, exact: true }) })
				.locator('td.counted')
		);

test('the pilots are ranked by their best flights, in a year or in all years', async ({
	page,
	keep,
	wide,
	problems
}) => {
	// The logbook has a third side where the site has a Venti server. On a
	// wide screen the page of everyone's flights has no switch: the ranking
	// is in the bar across the top.
	await page.goto('/logbook/everyone/');
	if (wide) {
		await page
			.getByRole('navigation', { name: 'Sections' })
			.getByRole('link', { name: 'Ranking' })
			.click();
	} else {
		await whose(page).getByRole('link', { name: 'Ranking' }).click();
	}
	await expect(page).toHaveURL(/\/logbook\/ranking\/$/);
	await expect(page).toHaveTitle('Ranking · Venti');
	await expect(whose(page).getByRole('link', { name: 'Ranking' })).toHaveAttribute(
		'aria-current',
		'page'
	);
	// The newest season is the chosen one: the year of today.
	const seasons = page.getByRole('radiogroup', { name: 'Season' }).getByRole('radio');
	await expect(seasons).toHaveCount(5);
	await expect(seasons.first()).toBeChecked();

	// All years: every public flight of the development mode counts.
	await page.getByRole('radio', { name: 'All years' }).check();
	await expect(page).toHaveURL(/\?season=All(%20|\+)years$/);
	await expect(pilots(page)).toHaveCount(RANKED);

	// Lena Probe's three flights give the most points.
	const first = pilots(page).first();
	await expect(first.getByRole('link', { name: 'Lena Probe' })).toBeVisible();
	await expect(first).toContainText('28.19');
	await expect(first).toContainText('3 flights');
	// Then Mara Muster: the flights that count, the best first, each with
	// its day, its distance and its points.
	await expect(pilots(page).nth(1)).toContainText('Mara Muster');
	await expect(pilots(page).nth(1)).toContainText('23.42');
	await expect(counted(page, 'Mara Muster')).toHaveCount(2);
	await expect(counted(page, 'Mara Muster').first()).toContainText('Sun, 16 Aug 2026');
	await expect(counted(page, 'Mara Muster').first()).toContainText('12.0 km');
	await expect(counted(page, 'Mara Muster').first()).toContainText('11.98');
	await expect(counted(page, 'Mara Muster').last()).toContainText('11.44');
	// Then Jonas Beispiel, and last Ines Vorlage with one flight.
	await expect(pilots(page).nth(2)).toContainText('Jonas Beispiel');
	await expect(pilots(page).nth(2)).toContainText('21.00');
	await expect(pilots(page).last()).toContainText('Ines Vorlage');
	await expect(pilots(page).last()).toContainText('1 flight');
	await expect(counted(page, 'Demo Pilot')).toHaveCount(1);
	// Each pilot with their face: the picture, or the initials.
	await expect(pilots(page).nth(2).locator('.face')).toHaveCount(1);
	// The rule, and whose reckoning it is, is said under the list.
	await expect(page.getByText(/best 3 flights shared with everyone, added up/)).toBeVisible();
	await expect(
		page.getByText(/It is not a league’s result|It is not a league's result/)
	).toBeVisible();

	// The address holds the season: opened again, the ranking is the same.
	await page.reload();
	await expect(page.getByRole('radio', { name: 'All years' })).toBeChecked();
	await expect(pilots(page)).toHaveCount(RANKED);

	// The oldest year offered: nobody flew then.
	await seasons.nth(3).check();
	await expect(page.getByRole('heading', { name: 'Nobody is ranked' })).toBeVisible();
	await expect(page.getByText(/Choose another year, or all years/)).toBeVisible();
	await keep('85-ranking-nobody');
	expect(problems).toEqual([]);
});

test('the ranking of a year names the year in its address, and leads to pilots and flights', async ({
	page,
	keep
}) => {
	await page.goto('/logbook/ranking/?season=2026');
	await expect(page.getByRole('radio', { name: '2026' })).toBeChecked();
	await expect(pilots(page)).toHaveCount(RANKED);
	await expect(pilots(page).first()).toContainText('Lena Probe');
	await keep('84-ranking');

	// A flight that counts opens that flight.
	await counted(page, 'Demo Pilot').first().getByRole('link').click();
	await expect(page).toHaveURL(/\/logbook\/everyone\/flight\/\?id=/);
	await page.goBack();

	// A pilot's name leads to that pilot's flights.
	await pilots(page).nth(2).getByRole('link', { name: 'Jonas Beispiel' }).click();
	await expect(page).toHaveURL(/\/logbook\/everyone\/pilot\/\?handle=jonas$/);
	await expect(page.getByRole('heading', { level: 1, name: 'Jonas Beispiel' })).toBeVisible();
});

test('a server that cannot rank says so, and can be asked again', async ({ context, page }) => {
	let down = true;
	await context.route('**/v1/ranking**', (route) =>
		down ? route.fulfill({ status: 503, body: '' }) : route.continue()
	);
	await page.goto('/logbook/ranking/?season=2026');

	await expect(
		page.getByRole('heading', { name: 'The ranking could not be fetched' })
	).toBeVisible();
	down = false;
	await page.getByRole('button', { name: 'Try again' }).click();
	await expect(pilots(page)).toHaveCount(RANKED);
});
