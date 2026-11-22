/**
 * The map tab: the pilot's own position when asked for, and the way
 * between the sections.
 */

import { addSampleFlight, expect, IN_THE_VALLEY, mapTabDrawn, test } from './helpers';

test('the map shows where the pilot is, on request', async ({
	context,
	page,
	mapDrawn,
	keep,
	problems
}) => {
	await context.grantPermissions(['geolocation']);
	await context.setGeolocation(IN_THE_VALLEY);
	await addSampleFlight(page);

	// A flight fills a phone's screen: back to the pilot's flights, then the map.
	await page.getByRole('link', { name: 'My flights', exact: true }).first().click();
	await page.getByRole('link', { name: 'Map', exact: true }).click();
	await expect(page).toHaveTitle('Map · Venti');
	await expect(page.getByRole('region', { name: 'Map' })).toBeVisible();
	await mapTabDrawn(page, mapDrawn);

	// The button switches following on, and says so.
	const locate = page.getByRole('button', { name: 'Show my position' });
	await expect(locate).toHaveAttribute('aria-pressed', 'false');
	await locate.click();
	await expect(locate).toHaveAttribute('aria-pressed', 'true');
	await mapDrawn();
	await page.waitForTimeout(800);
	await keep('16-map-position');

	// Dragging the map away ends the following; the button says that too.
	const map = (await page.locator('canvas.maplibregl-canvas').boundingBox())!;
	await page.mouse.move(map.x + map.width / 2, map.y + map.height / 3);
	await page.mouse.down();
	await page.mouse.move(map.x + map.width / 2 + 80, map.y + map.height / 3 + 60, { steps: 5 });
	await page.mouse.up();
	await expect(locate).toHaveAttribute('aria-pressed', 'false');

	expect(problems).toEqual([]);
});

test('the map says why when it may not use the position', async ({ page, keep }) => {
	// Permission is not granted: the browser refuses.
	await page.goto('/map/');
	const locate = page.getByRole('button', { name: 'Show my position' });
	await expect(page.locator('canvas.maplibregl-canvas')).toBeVisible();
	await locate.click();

	const notice = page.getByRole('dialog', { name: 'Position unavailable' });
	await expect(notice).toBeVisible();
	await expect(notice).toContainText('Venti is not allowed to use your position.');
	await keep('17-map-position-refused');
	await notice.getByRole('button', { name: 'OK' }).click();
	await expect(locate).toHaveAttribute('aria-pressed', 'false');
});

test('the navigation leads to every section, and says where one is', async ({ page, wide }) => {
	await page.goto('/map/');
	const navigation = page.getByRole('navigation', { name: 'Sections' });
	await expect(navigation.getByRole('link', { name: 'Map', exact: true })).toHaveAttribute(
		'aria-current',
		'page'
	);

	// No logbook: the flights are everyone's, with the pilot's own as "Mine".
	await expect(navigation.getByRole('link', { name: 'Logbook' })).toHaveCount(0);
	await navigation.getByRole('link', { name: 'Flights', exact: true }).click();
	await expect(page).toHaveTitle(/Venti$/);
	await expect(navigation.getByRole('link', { name: 'Flights', exact: true })).toHaveAttribute(
		'aria-current',
		'page'
	);
	await page
		.getByRole('navigation', { name: 'Whose flights' })
		.getByRole('link', { name: 'Mine' })
		.click();
	await expect(page).toHaveTitle('My flights · Venti');
	await expect(navigation.getByRole('link', { name: 'Flights', exact: true })).toHaveAttribute(
		'aria-current',
		'page'
	);

	// The settings, and the account under them, are one section.
	await navigation.getByRole('link', { name: 'Settings', exact: true }).click();
	await expect(page).toHaveTitle('Settings · Venti');
	await page
		.locator('.settings-page')
		.getByRole('link', { name: /^Account/ })
		.click();
	await expect(page).toHaveTitle('Account · Venti');
	await expect(navigation.getByRole('link', { name: 'Settings', exact: true })).toHaveAttribute(
		'aria-current',
		'page'
	);
	await page.getByRole('main').getByRole('link', { name: 'Settings' }).click();
	await expect(page).toHaveTitle('Settings · Venti');

	// A flight fills a narrow screen, so the bar steps aside there. A wide
	// screen keeps its bar.
	await addSampleFlight(page);
	await expect(page).toHaveTitle('Sat, 4 Jul 2026 · Venti');
	if (wide) await expect(navigation).toBeVisible();
	else await expect(navigation).toBeHidden();
});
