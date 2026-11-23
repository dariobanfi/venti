/**
 * Finding a pilot by name, what a pilot's public flights add up to, and the
 * ranking of one month, from the Venti server in its development mode:
 * Lena Probe has three public flights there, of May, June and July 2026,
 * two of them from Musterkogel West. Mara Muster has a picture, made up
 * by the development mode; the others have none. `playwright.config.ts`
 * starts the server.
 */

import { expect, rowsOf, signUp, test } from './helpers';

test('a pilot is found by name, and the pilot’s page says what the flights add up to', async ({
	page,
	keep,
	problems
}) => {
	await page.goto('/flights/');
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
	// The core's line beside the pilot's face, here the initials.
	await expect(found.first()).toHaveAccessibleName('Lena Probe (@lena), 3 public flights');
	await keep('64b-pilot-search');

	// A pilot found opens that pilot's page.
	await found.first().click();
	await expect(page).toHaveURL(/\/pilots\/lena\/$/);
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
	// What the numbers are made of. The flights' terms are not shown (the owner, 2026-11-23).
	await expect(totals.getByText(/^Of the flights this pilot shared with everyone/)).toBeVisible();
	await expect(page.getByText('Terms for these flights')).toHaveCount(0);
	await expect(page.getByText(/^These flights are their pilots/)).toHaveCount(0);
	// One season alone has the same numbers.
	await totals.getByRole('radio', { name: '2026' }).check();
	await expect(row('Points, best 3 flights')).toContainText('28.19');
	// The season narrows the list beside the totals to its flights.
	await expect(rowsOf(page, 'Flights of Lena Probe')).toHaveCount(3);
	await totals.getByRole('radio', { name: 'All years' }).check();
	await keep('64c-pilot-totals');

	// A row about one flight opens that flight.
	await row('Longest flight').getByRole('link').click();
	await expect(page).toHaveURL(/\/\/[^/]+\/flights\/[0-9a-z]{26}\//);
	await page.goBack();
	// A take-off opens its place: the site, the flights from it and its
	// pilots of all years, Lena Probe among them.
	await sites.getByRole('link', { name: 'Musterkogel West, 2 flights' }).click();
	await expect(page).toHaveURL(/\/places\/[-0-9.]+,[-0-9.]+\/\?site=venti%3A/);
	await expect(page.getByRole('heading', { level: 1, name: 'Musterkogel West' })).toBeVisible();
	await expect(rowsOf(page, 'Flights from here').first()).toBeVisible();
	await expect(page.getByRole('radio', { name: 'All years' })).toBeChecked();
	await expect(rowsOf(page, /^Ranking from here/).filter({ hasText: 'Lena Probe' })).toHaveCount(1);
	await expect(page.getByRole('region', { name: 'The site' })).toBeVisible();
	await keep('64f-pilot-place');
	expect(problems).toEqual([]);
});

test('the ranking of a month is chosen beside the years, and is in the address', async ({
	page,
	keep,
	problems
}) => {
	await page.goto('/ranking/?season=2026');
	const pilots = rowsOf(page, /^Ranking of /);
	await expect(pilots.first()).toContainText('Lena Probe');
	await expect(page.getByText(/^2026: \d+ flights$/)).toBeVisible();

	// Only the months that have a ranking are offered, in the core's names.
	const months = page.getByRole('group', { name: 'Months of 2026' }).getByRole('button');
	// The whole year stands first, in the core's words, and is the one shown.
	await expect(months.first()).toHaveText('All of 2026');
	await expect(months.first()).toHaveAttribute('aria-pressed', 'true');
	await expect(months.nth(1)).toHaveText('September');
	// The ranking's heading is the period's title.
	await expect(page.getByRole('heading', { name: '2026', exact: true, level: 2 })).toBeVisible();
	await expect(months.filter({ hasText: 'January' })).toHaveCount(0);

	await months.filter({ hasText: 'May' }).click();
	await expect(page).toHaveURL(/\/ranking\/\?month=2026-05$/);
	await expect(page).toHaveTitle('Ranking of May 2026 · Venti');
	await expect(months.filter({ hasText: 'May' })).toHaveAttribute('aria-pressed', 'true');
	await expect(months.first()).toHaveAttribute('aria-pressed', 'false');
	await expect(page.getByRole('heading', { name: 'May 2026', level: 2 })).toBeVisible();
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

test('a pilot’s picture stands beside their name wherever it is, and can be reported', async ({
	page,
	keep,
	problems
}) => {
	const PICTURE = '/v1/pictures/0000000000000000000000p001';
	await signUp(page);

	// In the search: Mara's picture, Lena's initials.
	await page.goto('/flights/');
	const field = page.getByRole('searchbox', { name: 'Pilot' });
	const found = page.getByRole('list', { name: 'Pilots found' }).getByRole('link');
	await field.fill('mara');
	await expect(found.first()).toContainText('Mara Muster');
	await expect(found.first().locator('img')).toHaveAttribute('src', PICTURE);
	await field.fill('lena');
	await expect(found.first()).toContainText('Lena Probe');
	await expect(found.first().locator('.face')).toHaveText('LP');
	await field.fill('');

	// Beside the name on everyone's flights.
	const mara = page.locator(`img[src="${PICTURE}"]`).first();
	await expect(mara).toBeVisible();
	await expect.poll(() => mara.evaluate((img: HTMLImageElement) => img.naturalWidth)).toBe(256);

	// On the pilot's page, large, with a report of the picture beside the page's.
	await page.goto('/pilots/mara/');
	await expect(page.getByRole('heading', { level: 1, name: 'Mara Muster' })).toBeVisible();
	await expect(page.locator('header .face.l img')).toHaveAttribute('src', PICTURE);
	await keep('64d-pilot-picture');
	await page.getByRole('button', { name: 'Report picture' }).click();
	const form = page.getByRole('dialog', { name: 'Report picture' });
	await form.getByRole('radio', { name: 'Pretends to be someone else' }).check();
	await keep('64e-report-picture');
	await form.getByRole('button', { name: 'Send report' }).click();
	await expect(form.getByRole('status')).toHaveText('Sent. Whoever runs Venti will read it.');

	// A pilot without a picture has their initials and no such report.
	await page.goto('/pilots/lena/');
	await expect(page.getByRole('heading', { level: 1, name: 'Lena Probe' })).toBeVisible();
	await expect(page.locator('header .face.l')).toHaveText('LP');
	await expect(page.getByRole('button', { name: 'Report picture' })).toBeHidden();
	expect(problems).toEqual([]);
});
