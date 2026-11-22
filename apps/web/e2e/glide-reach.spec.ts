/**
 * What a glide reaches from a place on the map: offered in a clicked
 * place's card, drawn as an area over the region's elevation tiles, and
 * said in the core's words in a card with the pilot's settings.
 */

import { expect, mapTabDrawn, test } from './helpers';

type Drawn = { outline: number; dashed: number };
const drawn = (page: import('@playwright/test').Page) =>
	page.evaluate(() => (globalThis as { ventiReach?: Drawn }).ventiReach);

test('a click near the Wallberg offers what a glide reaches from there, and draws the area', async ({
	page,
	mapDrawn,
	keep,
	wide,
	problems
}) => {
	// The wind's arrows, the sites' signs, the gliders and the cables (the
	// Wallbergbahn runs up to the summit) stay off the map, so that a click
	// on the summit is a click on the place.
	await page.addInitScript(() =>
		localStorage.setItem(
			'venti.map-layers-off',
			JSON.stringify(['wind', 'sites', 'pilots', 'cables'])
		)
	);
	await page.goto('/?lat=47.6642&lon=11.7967');
	await mapTabDrawn(page, mapDrawn);

	// The map's middle, above the navigation where it covers the map.
	const canvas = page.locator('canvas.maplibregl-canvas');
	const box = (await canvas.boundingBox())!;
	const covered = await page.locator('.navigation-height').evaluate((bar) => bar.clientHeight);
	await canvas.click({ position: { x: box.width / 2, y: (box.height - covered) / 2 } });
	const place = page.getByRole(wide ? 'complementary' : 'dialog', { name: /° N, .*° E$/ });
	await place.getByRole('button', { name: 'What a glide reaches from here' }).click();
	await expect(place).toBeHidden();

	const card = page.getByRole('region', { name: 'What a glide reaches' });
	const words = card.locator('[data-reach-words]');
	await expect(words).toHaveText(/^From \d+ m at a glide of 8/);
	await expect(card.getByLabel('Glide', { exact: true })).toHaveValue('8');
	await expect.poll(async () => (await drawn(page))?.outline ?? 0).toBeGreaterThan(3);

	// A start 2000 m high and a better glide each ask the core again,
	// without the server: a wider area.
	const start = card.getByLabel('Start, m');
	await start.fill('2000');
	await start.press('Enter');
	await expect(words).toHaveText(/^From 2000 m at a glide of 8 you reach/);
	const glide = card.getByLabel('Glide', { exact: true });
	await glide.fill('10');
	await glide.press('Enter');
	await expect(words).toHaveText(/^From 2000 m at a glide of 10 you reach/);
	await expect(card.getByRole('button', { name: 'On the ground' })).toBeVisible();
	await mapDrawn();
	await keep('215-glide-reach');

	// The same in the dark.
	await page.emulateMedia({ colorScheme: 'dark' });
	await mapDrawn();
	await keep('216-glide-reach-dark');
	await page.emulateMedia({ colorScheme: 'light' });

	// "Clear" takes the area off the map, and the card with it.
	await card.getByRole('button', { name: 'Clear' }).click();
	await expect(card).toBeHidden();
	await expect.poll(async () => (await drawn(page))?.outline).toBe(0);
	expect(problems).toEqual([]);
});
