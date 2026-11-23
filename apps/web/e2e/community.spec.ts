/**
 * The pilots near a place, on Community: made of public flights, from the
 * browser's position where the site may have it, else the place the home
 * page's weather is for, else the region's middle. The development server
 * has Jonas Beispiel, Mara Muster (with a picture) and the demo pilot near
 * the region's middle, around the Brauneck, and Lena Probe near the
 * made-up sites by Garmisch.
 */

import { expect, test } from './helpers';

/** The browser's memory of the weather's place, as `src/lib/home/place.ts` keeps it. */
const remembered = (at: { lat: number; lon: number }) =>
	`localStorage.setItem('venti.weather-place', ${JSON.stringify(
		JSON.stringify({ at, kind: 'chosen', name: 'Made up' })
	)})`;

test('the pilots near the region’s middle, each a card to the pilot’s page', async ({
	page,
	keep,
	problems
}) => {
	await page.goto('/community/');
	const near = page.getByRole('region', { name: 'Pilots near' });
	await expect(near).toContainText('Within 25 km, the last 12 months');
	const cards = near.getByRole('list', { name: 'Pilots near' }).getByRole('listitem');
	await expect(cards.filter({ hasText: 'Jonas Beispiel' })).toBeVisible();
	const mara = cards.filter({ hasText: 'Mara Muster' });
	await expect(mara.locator('img')).toHaveAttribute('src', /^\/v1\/pictures\//);
	await expect(mara).toContainText(/\d+ flights? from here/);
	await expect(cards.filter({ hasText: 'Jonas Beispiel' }).locator('.face')).toHaveText('JB');
	await near.scrollIntoViewIfNeeded();
	await keep('270d-community-pilots-near');

	await mara.getByRole('link', { name: 'Mara Muster' }).click();
	await expect(page).toHaveURL(/\/pilots\/mara\/$/);
	expect(problems).toEqual([]);
});

test('the pilots near the place the weather is for, and none where nobody flew', async ({
	page,
	keep,
	problems
}) => {
	// By Garmisch, where the development server has Lena's flights.
	await page.addInitScript(remembered({ lat: 47.3, lon: 10.9 }));
	await page.goto('/community/');
	const near = page.getByRole('region', { name: 'Pilots near' });
	const cards = near.getByRole('list', { name: 'Pilots near' }).getByRole('listitem');
	await expect(cards.first()).toContainText('Lena Probe');
	await expect(cards.first()).toContainText('3 flights from here · Musterkogel West');
	await expect(cards.first()).toContainText(/^.*2026: 3 flights/);

	// Far from anything flown: the core's sentence.
	await page.addInitScript(remembered({ lat: 52.52, lon: 13.4 }));
	await page.reload();
	await expect(near).toContainText('No pilot has shared a flight from near here yet.');
	await near.scrollIntoViewIfNeeded();
	await keep('270e-community-pilots-near-empty');
	expect(problems).toEqual([]);
});
