/**
 * Flying sites on the map, from the Venti server in its development mode:
 * the take-offs and landings OpenStreetMap had for the northern Alps on
 * 18 November 2026. `playwright.config.ts` starts the server.
 */

import type { Locator, Page } from '@playwright/test';
import { BETWEEN_THE_MOUNTAINS, expect, mapTabDrawn, metric, showTheValley, test } from './helpers';

/** Where OpenStreetMap has the take-off of the Jägerkamp. */
const JAEGERKAMP = { latitude: 47.6791413, longitude: 11.9051032 };

/** How many sites a map says it shows, for those who cannot see it. */
const shown = (page: Page) => page.getByText(/^Flying sites on this part of the map/);

/** A chosen site: a sheet on a narrow screen, a card beside the map on a wide one. */
function sitePanel(page: Page, wide: boolean, name: string): Locator {
	return page.getByRole(wide ? 'complementary' : 'dialog', { name });
}

const closeButton = (panel: Locator, wide: boolean) =>
	panel.getByRole('button', { name: wide ? 'Close' : 'Done' });

test('the map draws the flying sites the server has, lists them and opens one', async ({
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

	// A map of the whole northern Alps draws no sites: they would be a
	// carpet of dots. The list says how to get them.
	await expect(shown(page)).toBeHidden();
	await expect(page.getByText(/^Sites: /)).toBeHidden();
	await page.getByRole('button', { name: 'Flying sites' }).click();
	const list = page.getByRole('dialog', { name: 'Flying sites' });
	await expect(list).toContainText('No flying sites here');
	await expect(list).toContainText(
		'Zoom the map in on an area, and the sites its source knows there are listed.'
	);
	await keep('39-sites-zoomed-out');
	await list.getByRole('button', { name: 'Done' }).click();

	// A valley: the signs are there, and whose data they are is written on
	// the map. A phone's map is narrower and has to show more of the land
	// for the same signs.
	await showTheValley(page, mapDrawn, wide ? 1 : 2);
	await expect(page.getByText('Sites: OpenStreetMap contributors')).toBeVisible();
	await expect(shown(page)).toHaveText('Flying sites on this part of the map: 5.');
	// The map places the signs over a few frames and fades their names in.
	await page.waitForTimeout(1500);
	await keep('40-map-sites');

	// Everything the signs say, as a list: the named ones first, by name.
	await page.getByRole('button', { name: 'Flying sites' }).click();
	const rows = list.getByRole('listitem');
	await expect(rows.first()).toContainText('Fluggelände Jägerkamp/Nagelspitz');
	await expect(rows.first()).toContainText('Take-off · 1,560 m');
	await expect(rows.filter({ hasText: 'Landeplatz Jägerkamp/Nagelspitz' })).toContainText(
		'Landing'
	);
	// A site without a name is called what it is, and most say no more.
	await expect(rows.filter({ hasText: /Take-off\s*No more is known/ }).first()).toBeVisible();
	// What a sign on a map is worth is said wherever sites are listed.
	await expect(list).toContainText(
		'A site on the map is not permission to fly there. Ask the club or the owner, and look at the place.'
	);
	await expect(list).toContainText('© OpenStreetMap contributors');
	await expect(list).toContainText('As of Wed, 18 Nov 2026.');
	await keep('41-sites-list');

	// Looked up by a part of its name.
	await list.getByRole('searchbox', { name: 'Site' }).fill('nothing like this');
	await expect(list).toContainText('No results for “nothing like this”');
	await list.getByRole('searchbox', { name: 'Site' }).fill('JÄGERKAMP');
	await expect(rows).toHaveCount(2);
	await rows.first().getByRole('button').click();
	await expect(list).toBeHidden();

	// The site: what its source says about it, and what that is worth.
	const site = sitePanel(page, wide, 'Fluggelände Jägerkamp/Nagelspitz');
	await expect(site).toBeVisible();
	await expect(metric(site, 'What it is')).toHaveText('Take-off');
	await expect(metric(site, 'Height')).toHaveText(/^1,560\sm$/);
	await expect(metric(site, 'Wind it works in')).toHaveText('Not given');
	await expect(site.getByRole('list', { name: 'What its source marks it as' })).toHaveText(
		'Marked as an official site'
	);
	await expect(site.getByRole('link', { name: 'See or correct it at the source' })).toHaveAttribute(
		'href',
		'https://www.openstreetmap.org/way/1080986173'
	);
	await expect(site).toContainText('A site on the map is not permission to fly there.');
	// The source's own notice, with its addresses to follow, and its credit.
	await expect(site).toContainText("This data is OpenStreetMap's and is available under");
	await expect(
		site.getByRole('link', { name: 'https://www.openstreetmap.org/fixthemap' })
	).toHaveAttribute('href', 'https://www.openstreetmap.org/fixthemap');
	await expect(site.getByRole('link', { name: '© OpenStreetMap contributors' })).toHaveAttribute(
		'href',
		'https://www.openstreetmap.org/copyright'
	);
	await expect(site).toContainText('As of Wed, 18 Nov 2026.');
	await keep('42-site');
	await closeButton(site, wide).click();
	await expect(site).toBeHidden();

	// A take-off whose source says which wind it works in: in words, and as
	// a wedge on a small compass.
	await page.getByRole('button', { name: 'Flying sites' }).click();
	await list.getByRole('searchbox', { name: 'Site' }).fill('vogelsang ost');
	await expect(rows).toHaveCount(1);
	await expect(rows.first()).toContainText('Take-off · wind NE');
	await rows.first().getByRole('button').click();
	const vogelsang = sitePanel(page, wide, 'Vogelsang Ost');
	await expect(metric(vogelsang, 'Wind it works in')).toHaveText('NE');
	await expect(metric(vogelsang, 'Wind it works in').locator('svg path')).toHaveCount(1);
	await expect(vogelsang).toContainText('Marked as an official site');
	await keep('43-site-wind');
	await closeButton(vogelsang, wide).click();

	expect(problems).toEqual([]);
});

test('a site is opened by its sign, and a station still by its arrow', async ({
	context,
	page,
	mapDrawn,
	wide
}) => {
	// The pilot stands at the take-off, and the map goes there.
	await context.grantPermissions(['geolocation']);
	await context.setGeolocation(JAEGERKAMP);
	await page.goto('/');
	await mapTabDrawn(page, mapDrawn);
	await page.getByRole('button', { name: 'Show my position' }).click();
	await mapDrawn();
	await expect(shown(page)).toHaveText(/^Flying sites on this part of the map: [1-9]\d*\.$/);
	await page.waitForTimeout(800);

	// The sign is in the middle of what the navigation leaves of the map.
	const map = (await page.locator('canvas.maplibregl-canvas').boundingBox())!;
	const covered = await page.locator('.navigation-height').evaluate((bar) => bar.clientHeight);
	// A little beside the sign: a finger is not that exact.
	await page.mouse.click(map.x + map.width / 2 + 10, map.y + (map.height - covered) / 2 - 12);

	const site = sitePanel(page, wide, 'Fluggelände Jägerkamp/Nagelspitz');
	await expect(site).toBeVisible();
	await expect(metric(site, 'Height')).toHaveText(/^1,560\sm$/);

	// A click on the bare map chooses nothing.
	await closeButton(site, wide).click();
	await page.mouse.click(map.x + map.width / 2 + 150, map.y + map.height / 4);
	await expect(site).toBeHidden();
});

test('a site named in a script the app has no letters for is drawn all the same', async ({
	context,
	page,
	mapDrawn,
	wide
}) => {
	// The map's labels are set in font files the app brings along: real
	// letters for Latin, Greek and Cyrillic, and for every other script a
	// file that says "no glyphs here", which the build keeps inside a
	// script. The map asks for those too, and the site's content security
	// policy has to let it: a refused font would cost a map tile all its
	// labels. Any refusal fails this test, as it does every other.
	await context.route('**/v1/sites**', async (route) => {
		const response = await route.fetch();
		const answer = await response.json();
		for (const layer of answer.layers) {
			for (const site of layer.sites) site.name = 'Старт تجربة ทดสอบ სტარტი';
		}
		await route.fulfill({ response, json: answer });
	});
	await context.grantPermissions(['geolocation']);
	await context.setGeolocation(BETWEEN_THE_MOUNTAINS);
	await page.goto('/');
	await mapTabDrawn(page, mapDrawn);
	await showTheValley(page, mapDrawn, wide ? 1 : 2);
	await expect(shown(page)).toHaveText('Flying sites on this part of the map: 5.');
	// The map places the signs and their names over a few frames.
	await page.waitForTimeout(1500);

	await page.getByRole('button', { name: 'Flying sites' }).click();
	const list = page.getByRole('dialog', { name: 'Flying sites' });
	await expect(list.getByRole('listitem').first()).toContainText('Старт تجربة ทดสอบ სტარტი');
});

test('a server that cannot be asked for sites is named in the list, and the map draws none', async ({
	context,
	page,
	mapDrawn,
	keep,
	wide
}) => {
	await context.route('**/v1/sites**', (route) => route.abort());
	await context.grantPermissions(['geolocation']);
	await context.setGeolocation(BETWEEN_THE_MOUNTAINS);
	await page.goto('/');
	await mapTabDrawn(page, mapDrawn);
	await showTheValley(page, mapDrawn, wide ? 1 : 2);

	// The map says nothing: it simply has no signs, and credits nobody.
	await expect(page.getByText(/^Sites: /)).toBeHidden();
	await expect(shown(page)).toBeHidden();

	await page.getByRole('button', { name: 'Flying sites' }).click();
	const list = page.getByRole('dialog', { name: 'Flying sites' });
	await expect(list).toContainText('No flying sites here');
	await expect(list).toContainText('The server could not be reached for flying sites.');
	await keep('44-sites-unreachable');
});
