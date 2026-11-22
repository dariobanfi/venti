/**
 * The map as the design pack's map screen: chips along the top that switch
 * its layers, pins for the flying sites, and a site's sheet with what the
 * forecast says of it, from Venti in its development mode (a stormy day in
 * November).
 */

import type { Page } from '@playwright/test';
import { AERIAL_HOSTS } from './bundled-map';
import {
	addSampleFlight,
	BETWEEN_THE_MOUNTAINS,
	expect,
	mapTabDrawn,
	showTheValley,
	test,
	mapTool,
	openCredits,
	openLayers,
	closeLayers,
	layerSwitch,
	toolRow
} from './helpers';

const chip = (page: Page, name: string) =>
	page.getByRole('group', { name: 'Layers on the map' }).getByRole('button', { name });
/**
 * The short line that names the makers of the photographs that can show
 * over the Tegernsee valley at a valley's zoom (`aerial-credit.ts`).
 */
const PHOTOS = 'Photos: basemap.at · Bayerische Vermessungsverwaltung · EOX';
const shown = (page: Page) => page.getByText(/^Flying sites on this part of the map/);

test('four chips and one sheet switch the map’s layers, and the browser remembers them', async ({
	context,
	page,
	mapDrawn,
	keep,
	wide,
	problems
}) => {
	await context.grantPermissions(['geolocation']);
	await context.setGeolocation(BETWEEN_THE_MOUNTAINS);
	await page.goto('/map/');
	await mapTabDrawn(page, mapDrawn);
	await showTheValley(page, mapDrawn, wide ? 1 : 2);
	await expect(shown(page)).toHaveText('Flying sites on this part of the map: 5.');

	// Four chips, the layers switched most, and two round buttons.
	const chips = page.getByRole('group', { name: 'Layers on the map' }).getByRole('button');
	await expect(chips).toHaveText([/Forecast/, /Radar/, /Wind/, /Sites/]);
	for (const name of ['Wind', 'Sites']) {
		await expect(chip(page, name)).toHaveAttribute('aria-pressed', 'true');
	}
	await expect(page.locator('.controls > button')).toHaveCount(2);
	// The credits are closed from the start.
	await expect(page.locator('.credits')).toBeHidden();
	await page.waitForTimeout(1500);
	await keep('99-map-chips');

	// Everything else is in the one sheet: every layer as a row with a
	// switch, the chips' four in the same state, and the tools under them.
	const sheet = await openLayers(page);
	for (const name of ['Wind', 'Sites', 'Airspace', 'Pilots', 'Cables']) {
		await expect(layerSwitch(page, name)).toBeChecked();
	}
	for (const name of [
		'Forecast',
		'Radar',
		'Slopes',
		'Aerial view',
		'Climbs now',
		'Where thermals have been'
	]) {
		await expect(layerSwitch(page, name)).not.toBeChecked();
	}
	for (const name of ['Wind stations', 'Flying sites', 'In the air', 'Plan a route']) {
		await expect(toolRow(page, name)).toBeVisible();
	}
	// The two headings are the core's.
	await expect(sheet.getByRole('heading', { name: 'On the map' })).toBeVisible();
	await expect(sheet.getByRole('heading', { name: 'Lists and plans' })).toBeVisible();
	await keep('99b-map-layers-sheet');

	// Wind off by its row: the chip says so, and a reload keeps it off.
	await layerSwitch(page, 'Wind').uncheck();
	await closeLayers(page);
	await expect(chip(page, 'Wind')).toHaveAttribute('aria-pressed', 'false');
	await page.reload();
	await mapTabDrawn(page, mapDrawn);
	await expect(chip(page, 'Wind')).toHaveAttribute('aria-pressed', 'false');
	await chip(page, 'Wind').click();
	await expect(chip(page, 'Wind')).toHaveAttribute('aria-pressed', 'true');
	await openLayers(page);
	await expect(layerSwitch(page, 'Wind')).toBeChecked();
	await closeLayers(page);

	// One click on the map's "i" shows every credit, complete.
	await showTheValley(page, mapDrawn, wide ? 1 : 2);
	await openCredits(page);
	const credits = page.locator('.credits');
	await expect(credits).toContainText(/Wind: .*Deutscher Wetterdienst.*GeoSphere Austria/);
	await expect(credits).toContainText('Airspace: openAIP, not certified');
	await expect(credits).toContainText('Sites: OpenStreetMap contributors');
	await expect(page.locator('.maplibregl-ctrl-attrib-inner')).toContainText('OpenStreetMap');
	await page.waitForTimeout(800);
	await keep('99c-map-credits');
	expect(problems).toEqual([]);
});

test('the layers’ sheet lays photographs from above under the map, and the browser remembers it', async ({
	context,
	page,
	mapDrawn,
	keep,
	wide,
	problems
}) => {
	// Every photograph the map asks for. (The test browser answers each with
	// a square of green: `bundled-map.ts`.)
	const photographs: string[] = [];
	page.on('request', (request) => {
		const host = new URL(request.url()).hostname;
		if (AERIAL_HOSTS.includes(host)) photographs.push(host);
	});

	await context.grantPermissions(['geolocation']);
	await context.setGeolocation(BETWEEN_THE_MOUNTAINS);
	await page.goto('/map/');
	await mapTabDrawn(page, mapDrawn);
	await showTheValley(page, mapDrawn, wide ? 1 : 2);

	// The drawn map, as the app comes: nothing is asked of the photographs' hosts.
	// A row of the layers' sheet.
	await openLayers(page);
	const aerial = layerSwitch(page, 'Aerial view');
	await expect(aerial).not.toBeChecked();
	expect(photographs).toEqual([]);

	await aerial.check();
	await closeLayers(page);
	await mapDrawn();
	expect(new Set(photographs)).toContain('tiles.maps.eox.at');
	// Whoever made the photographs is named on the map while they show.
	await expect(page.locator('.maplibregl-ctrl-attrib-inner')).toContainText('EOX IT Services');
	// A wide map writes its credits out, with every maker's own wording as
	// the style's sources give it: MapLibre names each source whose layer
	// shows at this zoom, wherever its photographs are. It needs no short line.
	if (wide) {
		for (const wording of [
			'EOxCloudless',
			'©swisstopo',
			'Autonome Provinz Bozen - Südtirol',
			'Datenquelle: basemap.at',
			'Bayerische Vermessungsverwaltung'
		]) {
			await expect(page.locator('.maplibregl-ctrl-attrib-inner')).toContainText(wording);
		}
		await expect(page.getByText(/^Photos: /)).toHaveCount(0);
	} else {
		// A phone folds the map's own credits behind their button: a short
		// line names the makers of the photographs that can show. Zoomed out
		// below the regions' photographs, only the world's; one step in,
		// once the map has come to rest, the valley's makers too.
		await openCredits(page);
		await expect(page.getByText('Photos: EOX', { exact: true })).toBeVisible();
		const canvas = page.locator('canvas.maplibregl-canvas');
		await canvas.focus();
		await page.keyboard.press('Equal');
		await mapDrawn();
		await canvas.blur();
		await openCredits(page);
		await expect(page.getByText(PHOTOS)).toBeVisible();
	}
	await keep('101-map-aerial');

	// A reload keeps the view, and the button switches back to the drawn map.
	await page.reload();
	await mapTabDrawn(page, mapDrawn);
	await openLayers(page);
	await expect(aerial).toBeChecked();
	await aerial.uncheck();
	await closeLayers(page);
	await expect(page.locator('.maplibregl-ctrl-attrib-inner')).not.toContainText('EOX');
	// On the drawn map the short line is gone.
	await expect(page.getByText(/^Photos: /)).toHaveCount(0);
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
	await page.goto('/map/');
	await mapTabDrawn(page, mapDrawn);
	await showTheValley(page, mapDrawn, wide ? 1 : 2);

	await mapTool(page, 'Flying sites');
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

test('a flight’s page names the photographs too, and its buttons keep apart', async ({
	page,
	keep,
	wide
}) => {
	await addSampleFlight(page);
	const aerial = page.getByRole('button', { name: 'Aerial view' });
	await aerial.click();
	await expect(aerial).toHaveAttribute('aria-pressed', 'true');
	const photos = page.getByText(PHOTOS);
	if (wide) await expect(photos).toHaveCount(0);
	else await expect(photos).toBeVisible();
	await page.waitForTimeout(800);
	await keep('102-flight-aerial');

	// At a phone's width the button lies clear of the back and menu buttons
	// and of the panel.
	if (!wide) {
		const box = (await aerial.boundingBox())!;
		for (const other of [
			page.getByRole('link', { name: 'Logbook' }).first(),
			page.getByRole('button', { name: 'More' }).first(),
			// The flight's panel.
			page.locator('.dock .side').first()
		]) {
			const them = await other.boundingBox();
			if (!them) continue;
			const apart =
				box.x + box.width <= them.x ||
				them.x + them.width <= box.x ||
				box.y + box.height <= them.y ||
				them.y + them.height <= box.y;
			expect(apart).toBe(true);
		}
	}
	await aerial.click();
	await expect(photos).toHaveCount(0);
});
