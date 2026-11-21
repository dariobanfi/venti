/**
 * Others in the air, from the Open Glider Network: the development server
 * lists made-up aircraft over the Tegernsee (paragliders, a hang glider, a
 * glider, a tow plane), one paraglider right by the Wallberg. Its network
 * connects nowhere and says so; this test answers that it is connected,
 * made up, so that the map draws what the server lists.
 */

import { expect, mapTabDrawn, test } from './helpers';

test('the map draws the aircraft the Open Glider Network hears, and a click opens one', async ({
	page,
	mapDrawn,
	keep,
	wide,
	problems
}) => {
	await page.route(
		(address) => address.pathname === '/v1/live/others',
		async (route) => {
			const response = await route.fetch();
			const body = await response.json();
			// Made up: the development server's network is never connected.
			for (const layer of body.layers) layer.connected = true;
			await route.fulfill({ response, json: body });
		}
	);
	// The wind's arrows and the sites' signs stay off the map, so that the
	// screenshot shows the aircraft.
	await page.addInitScript(() =>
		localStorage.setItem('venti.map-layers-off', JSON.stringify(['wind', 'sites']))
	);
	await page.goto('/?lat=47.6642&lon=11.7967');
	await mapTabDrawn(page, mapDrawn);

	// The network's aircraft are drawn, and its credit stands with the map's.
	await expect
		.poll(() =>
			page.evaluate(() => (globalThis as { ventiOthers?: string[] }).ventiOthers?.length ?? 0)
		)
		.toBeGreaterThan(0);
	await expect(page.locator('.credits')).toContainText(
		'Positions: Open Glider Network, glidernet.org (ODbL)'
	);
	await expect(page.getByText(/ others? in the air\.$/)).toBeAttached();
	await mapDrawn();
	await keep('230-map-others');

	// A click on the paraglider by the Wallberg, in the middle of what the
	// navigation leaves of the map, opens its sheet, not the place's card.
	const canvas = page.locator('canvas.maplibregl-canvas');
	const box = (await canvas.boundingBox())!;
	const covered = await page.locator('.navigation-height').evaluate((bar) => bar.clientHeight);
	await canvas.click({ position: { x: box.width / 2, y: (box.height - covered) / 2 } });
	const sheet = page.getByRole(wide ? 'complementary' : 'dialog', { name: 'Paraglider' });
	await expect(sheet).toBeVisible();
	await expect(sheet.locator('[data-other-line]')).toHaveText(/^Paraglider at 2,?140 m/);
	await expect(sheet).toContainText('Positions: Open Glider Network, glidernet.org (ODbL)');
	await expect(sheet).toContainText('never use it to avoid a collision');
	await expect(page.getByRole(wide ? 'complementary' : 'dialog', { name: /° N, / })).toHaveCount(0);
	await keep('231-map-other');

	// The same in the dark.
	await page.emulateMedia({ colorScheme: 'dark' });
	await mapDrawn();
	await keep('232-map-other-dark');
	await page.emulateMedia({ colorScheme: 'light' });

	expect(problems).toEqual([]);
});
