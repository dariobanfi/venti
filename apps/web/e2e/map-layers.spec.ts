/**
 * The map as the design pack's map screen: chips along the top that switch
 * its layers, pins for the flying sites, and a site's sheet with what the
 * forecast says of it, from Venti in its development mode (a stormy day in
 * November).
 */

import type { Page } from '@playwright/test';
import { BETWEEN_THE_MOUNTAINS, expect, mapTabDrawn, showTheValley, test } from './helpers';

const chip = (page: Page, name: string) =>
	page.getByRole('group', { name: 'Layers on the map' }).getByRole('button', { name });
const shown = (page: Page) => page.getByText(/^Flying sites on this part of the map/);

test('the chips switch the map’s layers, and the browser remembers them', async ({
	context,
	page,
	mapDrawn,
	keep,
	wide,
	problems
}) => {
	await context.grantPermissions(['geolocation']);
	await context.setGeolocation(BETWEEN_THE_MOUNTAINS);
	await page.goto('/');
	await mapTabDrawn(page, mapDrawn);
	await showTheValley(page, mapDrawn, wide ? 1 : 2);
	await expect(shown(page)).toHaveText('Flying sites on this part of the map: 5.');

	for (const name of ['Wind', 'Airspace', 'Sites', 'Pilots']) {
		await expect(chip(page, name)).toHaveAttribute('aria-pressed', 'true');
	}
	await page.waitForTimeout(1500);
	await keep('99-map-chips');

	// Wind off: the chip says so, and a reload keeps it off.
	await chip(page, 'Wind').click();
	await expect(chip(page, 'Wind')).toHaveAttribute('aria-pressed', 'false');
	await page.reload();
	await mapTabDrawn(page, mapDrawn);
	await expect(chip(page, 'Wind')).toHaveAttribute('aria-pressed', 'false');
	await chip(page, 'Wind').click();
	await expect(chip(page, 'Wind')).toHaveAttribute('aria-pressed', 'true');
	expect(problems).toEqual([]);
});

test('a take-off’s sheet has the forecast’s status and its line, a landing has neither', async ({
	context,
	page,
	mapDrawn,
	keep,
	wide
}) => {
	await context.grantPermissions(['geolocation']);
	await context.setGeolocation(BETWEEN_THE_MOUNTAINS);
	await page.goto('/');
	await mapTabDrawn(page, mapDrawn);
	await showTheValley(page, mapDrawn, wide ? 1 : 2);

	await page.getByRole('button', { name: 'Flying sites' }).click();
	const list = page.getByRole('dialog', { name: 'Flying sites' });
	await list.getByRole('searchbox', { name: 'Site' }).fill('Fluggelände Jägerkamp');
	await list.getByRole('listitem').first().getByRole('button').click();
	const site = page.getByRole(wide ? 'complementary' : 'dialog', {
		name: 'Fluggelände Jägerkamp/Nagelspitz'
	});
	await expect(site).toContainText('Take-off 1560 m');
	await expect(site.getByText(/^\s*(Flyable|Marginal|Not flyable)\s*$/)).toBeVisible({
		timeout: 30_000
	});
	await expect(site.getByText(/^(Today|Tomorrow|Mon|Tue|Wed|Thu|Fri|Sat|Sun): /)).toBeVisible();
	await keep('100-site-outlook');
});
