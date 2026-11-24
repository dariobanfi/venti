/**
 * Searching public flights as XContest's "Worldwide flights search" does,
 * against the Venti server in its development mode: eleven public flights,
 * five of them taking off at the Wallberg above the Tegernsee (Germany) and
 * six at the made-up sites south of the Zugspitze (Austria), some 75 km
 * apart. No flight there has a glider class. `playwright.config.ts` starts
 * the server.
 *
 * The area's map opens over the Alps at 47.45 N, 11.6 E, zoom 7.4: the
 * Wallberg lies some 28 km north-east of its middle, the Austrian sites
 * some 55 km west-south-west.
 */

import type { Page } from '@playwright/test';
import { expect, rowsOf, test } from './helpers';

/** How many public flights the development mode holds. */
const ALL = 11;
/** How many take off at the Wallberg. */
const WALLBERG = 5;

const rows = (page: Page) => rowsOf(page, 'Everyone’s flights');
const filter = (page: Page) => page.getByRole('button', { name: 'Filter', exact: true });
const chip = (page: Page, words: string) =>
	page.getByRole('button', { name: `Remove ${words}`, exact: true });
const sheet = (page: Page) => page.getByRole('dialog', { name: 'Area' });

test('a radius around a place on the map shows only the flights that took off inside it', async ({
	page,
	keep,
	problems
}) => {
	// A search in the address is read back when the page opens: a link shares it.
	await page.goto('/flights/?near=47.6642,11.7967&radius_km=5');
	await expect(rows(page)).toHaveCount(WALLBERG);
	await expect(chip(page, 'Within 5 km')).toBeVisible();
	await expect(rows(page).first()).toContainText('Jonas Beispiel');
	// The × lets go of it, and the address with it.
	await chip(page, 'Within 5 km').click();
	await expect(rows(page)).toHaveCount(ALL);
	await expect(page).toHaveURL(/\/flights\/?$/);

	// On the map: the mark in the middle, the radius on a slider.
	await filter(page).click();
	await page.getByRole('button', { name: 'Area', exact: true }).click();
	await expect(sheet(page)).toBeVisible();
	await expect(sheet(page).getByRole('radio', { name: 'Radius' })).toBeChecked();
	const radius = sheet(page).getByRole('slider', { name: 'Radius' });
	await radius.fill('40');
	await expect(sheet(page).getByText('40 km', { exact: true })).toBeVisible();
	await expect(sheet(page).locator('[aria-busy="false"]')).toBeVisible();
	await keep('61s-flights-area-radius');
	await sheet(page).getByRole('button', { name: 'Use this area' }).click();
	await expect(sheet(page)).toBeHidden();
	await expect(rows(page)).toHaveCount(WALLBERG);
	await expect(chip(page, 'Within 40 km')).toBeVisible();
	await expect(page).toHaveURL(/near=47\.45%2C11\.6&radius_km=40/);
	for (const name of ['Lena Probe', 'Timo Entwurf']) {
		await expect(rows(page).filter({ hasText: name })).toHaveCount(0);
	}
	await keep('61t-flights-searched');
	expect(problems).toEqual([]);
});

test('an area drawn on the map shows only the flights that took off inside it', async ({
	page,
	keep,
	problems
}) => {
	await page.goto('/flights/');
	await expect(rows(page)).toHaveCount(ALL);
	await filter(page).click();
	await page.getByRole('button', { name: 'Area', exact: true }).click();
	await sheet(page).getByText('Draw an area').click();
	await expect(sheet(page).getByText('Tap the corners on the map.')).toBeVisible();

	// Four corners around the Wallberg, some 24 pixels east and 38 north of
	// the middle, clear of the sites 84 west and 27 south of it: a degree of
	// latitude is some 178 pixels at this zoom.
	const canvas = sheet(page).locator('canvas');
	await expect(sheet(page).locator('[aria-busy="false"]')).toBeVisible();
	const box = (await canvas.boundingBox())!;
	const middle = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
	const corners = [
		[-20, -90],
		[80, -90],
		[80, 10],
		[-20, 10]
	];
	for (const [index, [x, y]] of corners.entries()) {
		await page.mouse.click(middle.x + x, middle.y + y);
		await expect(sheet(page).getByRole('status')).toHaveText(`${index + 1} of at most 50 corners`);
	}
	await sheet(page).getByRole('button', { name: 'Close the shape' }).click();
	await expect(sheet(page).getByRole('status')).toHaveText('4 corners');
	await expect(sheet(page).locator('[aria-busy="false"]')).toBeVisible();
	await keep('61u-flights-area-drawn');
	await sheet(page).getByRole('button', { name: 'Use this area' }).click();
	await expect(rows(page)).toHaveCount(WALLBERG);
	await expect(chip(page, 'Drawn area')).toBeVisible();
	await expect(page).toHaveURL(/area=/);

	// The address carries the area: the same list after a reload.
	await page.reload();
	await expect(rows(page)).toHaveCount(WALLBERG);
	await expect(chip(page, 'Drawn area')).toBeVisible();
	expect(problems).toEqual([]);
});

test('a glider class, a kind and a least distance narrow the list, each a chip', async ({
	page,
	keep,
	problems
}) => {
	await page.goto('/flights/');
	await expect(rows(page)).toHaveCount(ALL);
	await filter(page).click();

	// No flight of the development mode has a class: nothing matches.
	await page.getByLabel('Glider', { exact: true }).selectOption({ label: 'EN C' });
	await expect(chip(page, 'EN C')).toBeVisible();
	await expect(page).toHaveURL(/class=en_c/);
	await expect(page.getByRole('heading', { name: 'No flights match' })).toBeVisible();
	await keep('61v-flights-search-empty');
	await chip(page, 'EN C').click();
	await expect(rows(page)).toHaveCount(ALL);
	await expect(page.getByLabel('Glider', { exact: true })).toHaveValue('');

	// Every one of them is a free flight; none is an FAI triangle.
	await page.getByLabel('Kind', { exact: true }).selectOption({ label: 'FAI triangle' });
	await expect(chip(page, 'FAI triangle')).toBeVisible();
	await expect(page.getByRole('heading', { name: 'No flights match' })).toBeVisible();
	await page.getByLabel('Kind', { exact: true }).selectOption({ label: 'Free flight' });
	await expect(chip(page, 'Free flight')).toBeVisible();
	await expect(rows(page)).toHaveCount(ALL);

	// At least 10 km: fewer flights, with the kind still chosen.
	await page.getByLabel('At least').fill('10');
	await page.getByLabel('At least').press('Enter');
	await expect(chip(page, '10 km or more')).toBeVisible();
	await expect(page).toHaveURL(/kind=free_flight&min_km=10/);
	// Seven of them: the four short ones are left out.
	await expect(rows(page)).toHaveCount(7);
	await keep('61w-flights-search-chips');
	expect(problems).toEqual([]);
});

test('the ranking is of the flights that took off in a country, or worldwide', async ({
	page,
	keep,
	problems
}) => {
	const pilots = rowsOf(page, /^Ranking of /);
	await page.goto('/ranking/');
	await expect(pilots).toHaveCount(6);
	await filter(page).click();
	await page.getByLabel('Take-off in').selectOption({ label: 'Austria' });
	await expect(chip(page, 'AT')).toBeVisible();
	await expect(page).toHaveURL(/\/ranking\/?\?country=AT$/);
	await expect(pilots.filter({ hasText: 'Lena Probe' })).toHaveCount(1);
	await expect(pilots.filter({ hasText: 'Mara Muster' })).toHaveCount(0);
	await keep('84s-ranking-country');
	await page.getByLabel('Take-off in').selectOption({ label: 'Germany' });
	await expect(chip(page, 'DE')).toBeVisible();
	await expect(pilots.filter({ hasText: 'Mara Muster' })).toHaveCount(1);
	await expect(pilots.filter({ hasText: 'Lena Probe' })).toHaveCount(0);
	// Worldwide again.
	await page.getByLabel('Take-off in').selectOption({ label: 'Worldwide' });
	await expect(pilots).toHaveCount(6);
	await expect(page).toHaveURL(/\/ranking\/?$/);
	expect(problems).toEqual([]);
});

test('a pilot’s flights are searched the same way', async ({ page, problems }) => {
	await page.goto('/pilots/lena/?country=DE');
	await expect(chip(page, 'DE')).toBeVisible();
	await expect(page.getByRole('heading', { name: 'No flights match' })).toBeVisible();
	await chip(page, 'DE').click();
	await expect(rowsOf(page, /^Flights of /)).not.toHaveCount(0);
	await expect(page).toHaveURL(/\/pilots\/lena\/?$/);
	expect(problems).toEqual([]);
});
