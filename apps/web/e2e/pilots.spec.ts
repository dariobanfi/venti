/**
 * Finding a pilot by name, what a pilot's public flights add up to, and the
 * ranking of one month, from the Venti server in its development mode:
 * Lena Probe has three public flights there, of May, June and July 2026,
 * two of them from Musterkogel West. `playwright.config.ts` starts the
 * server.
 */

import { expect, rowsOf, test } from './helpers';

test('a pilot is found by name, and the pilot’s page says what the flights add up to', async ({
	page,
	keep,
	problems
}) => {
	await page.goto('/logbook/everyone/');
	const field = page.getByRole('searchbox', { name: 'Pilot' });
	const found = page.getByRole('list', { name: 'Pilots found' }).getByRole('link');

	// One letter is not asked for: the core says why.
	await field.fill('l');
	await expect(page.getByText(/^Type at least two letters/)).toBeVisible();
	await expect(found).toHaveCount(0);

	// Nobody of that name: the core's line.
	await field.fill('zzzz');
	await expect(
		page.getByText('No pilot by this name has shared a flight with everyone.')
	).toBeVisible();

	// Two letters: the pilots whose name begins with them first.
	await field.fill('le');
	await expect(found.first()).toHaveText('Lena Probe (@lena), 3 public flights');
	await keep('64b-pilot-search');

	// A pilot found opens that pilot's page.
	await found.first().click();
	await expect(page).toHaveURL(/\/logbook\/everyone\/pilot\/\?handle=lena$/);
	await expect(page.getByRole('heading', { level: 1, name: 'Lena Probe' })).toBeVisible();
	await expect(rowsOf(page, 'Flights of Lena Probe')).toHaveCount(3);

	// What the flights add up to, as the core's rows.
	const totals = page.getByRole('region', { name: 'Totals' });
	const row = (label: string) => totals.locator('.row', { hasText: label });
	await expect(row('Flights').first()).toContainText('3');
	await expect(row('Points, best 3 flights')).toContainText('28.19');
	await expect(row('Distance scored')).toContainText('28.2 km');
	// The take-offs flown from most, and the twelve months.
	const sites = totals.getByRole('list', { name: 'Take-offs flown from most' });
	await expect(sites.getByRole('listitem').first()).toHaveText('Musterkogel West, 2 flights');
	await expect(totals.getByRole('group', { name: 'Flights by month' }).locator('li')).toHaveCount(
		12
	);
	await expect(totals.getByText('May: 1 flight')).toBeAttached();
	// What the numbers are made of, and under what the flights are given.
	await expect(totals.getByText(/^Of the flights this pilot shared with everyone/)).toBeVisible();
	await expect(totals.getByText(/^These flights are their pilots/)).toBeVisible();
	// One season alone has the same numbers.
	await totals.getByRole('radio', { name: '2026' }).check();
	await expect(row('Points, best 3 flights')).toContainText('28.19');
	await totals.getByRole('radio', { name: 'All years' }).check();
	await keep('64c-pilot-totals');

	// A row about one flight opens that flight.
	await row('Longest flight').getByRole('link').click();
	await expect(page).toHaveURL(/\/logbook\/everyone\/flight\/\?id=/);
	await page.goBack();
	// A take-off opens its place.
	await sites.getByRole('link', { name: 'Musterkogel West, 2 flights' }).click();
	await expect(page).toHaveURL(/\/logbook\/everyone\/place\/\?.*site=/);
	expect(problems).toEqual([]);
});

test('the ranking of a month is chosen beside the years, and is in the address', async ({
	page,
	keep,
	problems
}) => {
	await page.goto('/logbook/ranking/?season=2026');
	const pilots = rowsOf(page, /^Ranking of /);
	await expect(pilots.first()).toContainText('Lena Probe');
	await expect(page.getByText(/^2026: \d+ flights$/)).toBeVisible();

	// Only the months that have a ranking are offered, in the core's names.
	const months = page.getByRole('group', { name: 'Months of 2026' }).getByRole('button');
	await expect(months.first()).toHaveText('September');
	await expect(months.filter({ hasText: 'January' })).toHaveCount(0);

	await months.filter({ hasText: 'May' }).click();
	await expect(page).toHaveURL(/\/logbook\/ranking\/\?month=2026-05$/);
	await expect(page).toHaveTitle('Ranking of May 2026 · Venti');
	await expect(months.filter({ hasText: 'May' })).toHaveAttribute('aria-pressed', 'true');
	await expect(page.getByText(/^May 2026: \d+ flights?$/)).toBeVisible();
	await expect(rowsOf(page, 'Ranking of May 2026').first()).toContainText('Lena Probe');
	await expect(page.getByRole('radio', { name: '2026' })).toBeChecked();
	await keep('84b-ranking-month');

	// The address holds the month: opened again, the ranking is the same.
	await page.reload();
	await expect(rowsOf(page, 'Ranking of May 2026').first()).toContainText('Lena Probe');
	await expect(months.filter({ hasText: 'May' })).toHaveAttribute('aria-pressed', 'true');

	// Pressed again, the month gives way to its whole year.
	await months.filter({ hasText: 'May' }).click();
	await expect(page).not.toHaveURL(/month=/);
	await expect(page.getByText(/^2026: \d+ flights$/)).toBeVisible();
	await expect(page.getByRole('radio', { name: '2026' })).toBeChecked();
	expect(problems).toEqual([]);
});
