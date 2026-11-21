/**
 * The map of slopes: the "Slopes" chip, off until switched on, draws how
 * steep the ground is in the core's five classes over the region's
 * elevation tiles once the map is close enough, with its key.
 */

import { expect, mapTabDrawn, test } from './helpers';

type Drawn = { drawn: boolean; cells: number; classed: number; byClass: number[] };
const drawn = (page: import('@playwright/test').Page) =>
	page.evaluate(() => (globalThis as { ventiSlopes?: Drawn }).ventiSlopes);

test('the slopes around the Wallberg are drawn once the map is close, with their key', async ({
	page,
	mapDrawn,
	keep,
	problems
}) => {
	// The wind's arrows, the sites' signs and the gliders stay off the map,
	// so that the screenshots show the slopes.
	await page.addInitScript(() =>
		localStorage.setItem('venti.map-layers-off', JSON.stringify(['wind', 'sites', 'pilots']))
	);
	await page.goto('/?lat=47.6642&lon=11.7967');
	await mapTabDrawn(page, mapDrawn);

	// Off until the pilot switches it on; at the valley's zoom it asks to come closer.
	const chip = page.getByRole('button', { name: 'Slopes', exact: true });
	await expect(chip).toHaveAttribute('aria-pressed', 'false');
	await chip.click();
	await expect(chip).toHaveAttribute('aria-pressed', 'true');
	await expect(page.getByText('Zoom in to a hillside to see how steep it is.')).toBeVisible();
	expect((await drawn(page))?.drawn ?? false).toBe(false);

	// From 10.5 to 13.5 with the keyboard, a step at a time (a phone has
	// no zoom buttons): the layer is asked for once the map has rested.
	const map = page.locator('canvas.maplibregl-canvas');
	await map.focus();
	for (let step = 0; step < 3; step += 1) {
		await page.keyboard.press('Equal');
		await mapDrawn();
	}
	// The screenshots show the map, not the ring of the keyboard's focus.
	await map.blur();
	await expect.poll(async () => (await drawn(page))?.drawn ?? false).toBe(true);
	await expect(page.getByText('Zoom in to a hillside to see how steep it is.')).toBeHidden();
	const slopes = (await drawn(page))!;
	expect(slopes.cells).toBeGreaterThan(1000);
	expect(slopes.cells).toBeLessThanOrEqual(40_000);
	// The Wallberg has the valley's gentle floor and slopes of 35° and more.
	expect(slopes.byClass[0]).toBeGreaterThan(0);
	expect(slopes.byClass[3] + slopes.byClass[4]).toBeGreaterThan(0);

	const key = page.getByRole('region', { name: 'Slopes' });
	await expect(key).toBeVisible();
	for (const words of ['Under 25°', '25–30°', '30–35°', '35–40°', '40° and over']) {
		await expect(key.getByText(words, { exact: true })).toBeVisible();
	}
	await expect(key).toContainText('Steeper than 30° is where snow slides; on foot, take care.');
	await mapDrawn();
	await keep('249-map-slopes');

	// Kept by the browser: still on after a reload.
	await page.reload();
	await mapTabDrawn(page, mapDrawn);
	await expect(chip).toHaveAttribute('aria-pressed', 'true');

	// Off again: nothing drawn, no key.
	await chip.click();
	await expect(chip).toHaveAttribute('aria-pressed', 'false');
	await expect.poll(async () => (await drawn(page))?.drawn ?? false).toBe(false);
	await expect(key).toBeHidden();
	expect(problems).toEqual([]);
});
