/**
 * Flying sites on the map, from the Venti server in its development mode:
 * the take-offs and landings OpenStreetMap had for the northern Alps on
 * 18 November 2026. `playwright.config.ts` starts the server.
 */

import type { Locator, Page } from '@playwright/test';
import {
	BETWEEN_THE_MOUNTAINS,
	expect,
	mapTabDrawn,
	metric,
	showMyPosition,
	showTheValley,
	test
} from './helpers';

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
	await expect(rows.first()).toContainText('Take-off · 1560 m');
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
	await expect(metric(site, 'Height')).toHaveText(/^1560\sm$/);
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
	// What its source says beyond that, in the core's sentence, and its web
	// page, which is somebody else's and opens outside the app.
	const more = site.getByRole('region', { name: 'What its source says about it' });
	await expect(more).toContainText('A fee is asked for using this site.');
	const webPage = more.getByRole('link', { name: /^Web page/ });
	await expect(webPage).toContainText('www.dhv.de');
	await expect(webPage).toHaveAttribute('href', 'https://www.dhv.de/');
	await expect(webPage).toHaveAttribute('target', '_blank');
	await expect(webPage).toHaveAttribute('rel', 'noopener noreferrer');
	// Kept as the pilot's own, and forgotten again, by the same row.
	await site.getByRole('button', { name: 'Keep as my take-off' }).click();
	await expect(site.getByRole('button', { name: 'Kept' })).toHaveAttribute('aria-pressed', 'true');
	await keep('42-site');
	await site.getByRole('button', { name: 'Kept' }).click();
	await expect(site.getByRole('button', { name: 'Keep as my take-off' })).toHaveAttribute(
		'aria-pressed',
		'false'
	);
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
	await showMyPosition(page, mapDrawn);
	await expect(shown(page)).toHaveText(/^Flying sites on this part of the map: [1-9]\d*\.$/);
	await page.waitForTimeout(800);

	// The sign is in the middle of what the navigation leaves of the map.
	const map = (await page.locator('canvas.maplibregl-canvas').boundingBox())!;
	const covered = await page.locator('.navigation-height').evaluate((bar) => bar.clientHeight);
	// A little beside the sign: a finger is not that exact.
	await page.mouse.click(map.x + map.width / 2 + 10, map.y + (map.height - covered) / 2 - 12);

	const site = sitePanel(page, wide, 'Fluggelände Jägerkamp/Nagelspitz');
	await expect(site).toBeVisible();
	await expect(metric(site, 'Height')).toHaveText(/^1560\sm$/);

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
	await expect(list).toContainText('Venti could not be reached for flying sites.');
	await keep('44-sites-unreachable');
});

/** What the map tab last drew of the sites, which a map cannot be asked itself. */
const drawn = (page: Page) =>
	page.evaluate(
		() =>
			(
				globalThis as unknown as {
					ventiSites: { signs: string[]; apart: string[]; areas: string[] };
				}
			).ventiSites
	);

const JAEGERKAMP_TAKEOFF = 'osm-free-flying:way/1080986173';
const JAEGERKAMP_LANDING = 'osm-free-flying:way/1080988126';

test('the sites are filtered by a wind: what fits, what stands apart, and what is left out', async ({
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

	// The filter's choices: a pane at the map's side on a wide screen, a sheet
	// of half the screen on a narrow one, the map staying in view above it.
	const opens = page.getByRole('button', { name: 'Filter sites', exact: true });
	await expect(opens).toHaveAttribute('aria-pressed', 'false');
	await opens.click();
	const filter = page.getByRole(wide ? 'complementary' : 'dialog', { name: 'Filter sites' });
	// Nothing is asked yet, and the core says so.
	await expect(filter.getByRole('status')).toContainText('All sites');
	await expect(filter.getByRole('status')).toContainText(/^All sites\s*\d+ sites\s*$/);
	const show = filter.getByRole('radiogroup', { name: 'Show' });
	await expect(show.getByRole('radio')).toHaveCount(3);
	await expect(show.getByRole('radio', { name: 'All sites' })).toBeChecked();
	// The sixteen points of the compass after "Any", and the wind a station
	// measures now where the map has one; no forecast, which the page has not.
	const winds = filter.getByRole('group', { name: 'Take-offs for a wind from' });
	await expect(winds.getByRole('button', { name: 'Any' })).toHaveAttribute('aria-pressed', 'true');
	await expect(winds.getByRole('button', { name: 'NNW' })).toBeVisible();
	await expect(winds.getByRole('button', { name: 'Forecast now' })).toHaveCount(0);
	await expect(filter.getByRole('radiogroup', { name: 'Take-offs from' })).toContainText(
		/Any\s*1000 m\s*1500 m\s*2000 m/
	);
	await expect(filter).toContainText('A site on the map is not permission to fly there.');

	// Take-offs for a north-east wind: the Vogelsang's east side names it,
	// the Jägerkamp and the Wallberg's ramps name no wind at all.
	await show.getByText('Take-offs', { exact: true }).click();
	await winds.getByRole('button', { name: 'NE', exact: true }).click();
	const said = filter.getByRole('status');
	await expect(said).toContainText('Take-offs for a wind from NE');
	await expect(said).toContainText(/[1-9]\d* of \d+ take-offs fits? a wind from NE/);
	await expect(said).toContainText(/[1-9]\d* take-offs give no wind directions/);
	const [, fits, takeoffs] = /(\d+) of (\d+) take-offs/.exec(await said.innerText())!;
	const [, unsaid] = /(\d+) take-offs give no wind directions/.exec(await said.innerText())!;
	// Some fit, some say nothing, and the rest name another wind: left out.
	expect(Number(fits) + Number(unsaid)).toBeLessThan(Number(takeoffs));

	// The map draws those that fit as they were, those that say nothing as
	// muted signs, and no landing.
	await expect.poll(async () => (await drawn(page)).signs.length).toBe(Number(fits));
	const onMap = await drawn(page);
	expect(onMap.apart).toHaveLength(Number(unsaid));
	expect(onMap.apart).toContain(JAEGERKAMP_TAKEOFF);
	expect([...onMap.signs, ...onMap.apart]).not.toContain(JAEGERKAMP_LANDING);
	// Three take-offs are in view, none of them names a wind; the two landings are gone.
	await expect(shown(page)).toHaveText('Flying sites on this part of the map: 3.');
	await expect(opens).toHaveAttribute('aria-pressed', 'true');
	await page.waitForTimeout(1500);
	await keep('142-sites-filter');

	// By height instead: the Jägerkamp's take-off gives its own, 1560 m, and
	// fits; the Wallberg's ramps give none and stand apart, as muted signs.
	await winds.getByRole('button', { name: 'Any' }).click();
	const heights = filter.getByRole('radiogroup', { name: 'Take-offs from' });
	await heights.getByText('1500 m', { exact: true }).click();
	await expect(said).toContainText('Take-offs from 1500 m up');
	await expect(said).toContainText(/[1-9]\d* of \d+ take-offs lies? at 1500 m or higher/);
	await expect(said).toContainText(/[1-9]\d* take-offs give no height/);
	await expect.poll(async () => (await drawn(page)).signs).toContain(JAEGERKAMP_TAKEOFF);
	expect((await drawn(page)).apart).toContain('osm-free-flying:node/5964676196');
	// The take-off is an area in its source, and has its outline with its sign.
	expect((await drawn(page)).areas).toContain(JAEGERKAMP_TAKEOFF);
	const close = filter.getByRole('button', { name: wide ? 'Close' : 'Done' });
	await close.click();
	await expect(filter).toBeHidden();
	await page.waitForTimeout(1500);
	await keep('143-map-sites-filtered');

	// Closed, the filter stays said on the map, in the core's words, as a row
	// that opens the choices again.
	await page.getByRole('button', { name: /^Take-offs from 1500 m up/ }).click();
	await heights.getByText('Any', { exact: true }).click();
	await winds.getByRole('button', { name: 'NE', exact: true }).click();
	await expect(said).toContainText(`${fits} of ${takeoffs} take-offs`);
	await close.click();
	await expect(filter).toBeHidden();
	const on = page.getByRole('button', { name: /^Take-offs for a wind from NE/ });
	await expect(on).toContainText(`${fits} of ${takeoffs} take-offs`);
	await expect(on).toContainText(`${unsaid} take-offs give no wind directions`);

	// The list has the same filter: those that fit first, then the take-offs
	// whose source does not say, under the core's heading and sentence.
	await page.getByRole('button', { name: 'Flying sites', exact: true }).click();
	const list = page.getByRole('dialog', { name: 'Flying sites' });
	await expect(list.getByRole('button', { name: 'Take-offs for a wind from NE' })).toBeVisible();
	await expect(list.getByRole('status')).toHaveText(
		new RegExp(`^${fits} of ${takeoffs} take-offs fits? a wind from NE$`)
	);
	const fitting = list.getByRole('list', { name: 'Flying sites' }).getByRole('listitem');
	await expect(fitting).toHaveCount(Number(fits));
	await expect(fitting.filter({ hasText: 'Vogelsang Ost' })).toContainText('wind NE');
	const apartTitle = `${unsaid} take-offs give no wind directions`;
	await expect(list.getByRole('heading', { name: apartTitle })).toBeVisible();
	await expect(list).toContainText(
		'Their source does not say, which says nothing about them: they may fit or not. They are shown apart, not left out.'
	);
	const apart = list.getByRole('list', { name: apartTitle }).getByRole('listitem');
	await expect(apart).toHaveCount(Number(unsaid));
	await expect(apart.filter({ hasText: 'Fluggelände Jägerkamp/Nagelspitz' })).toBeVisible();
	await expect(list.getByText('Landeplatz Jägerkamp/Nagelspitz')).toHaveCount(0);
	await keep('144-sites-list-filtered');

	// The filter is changed from the list too. Landings are asked neither a
	// wind nor a height, and neither is offered there.
	await list.getByRole('button', { name: 'Take-offs for a wind from NE' }).click();
	await list
		.getByRole('radiogroup', { name: 'Show' })
		.getByText('Landings', { exact: true })
		.click();
	await expect(list.getByRole('group', { name: 'Take-offs for a wind from' })).toHaveCount(0);
	await expect(list.getByRole('radiogroup', { name: 'Take-offs from' })).toHaveCount(0);
	await expect(list.getByRole('status')).toContainText(/Landings\s*[1-9]\d* landings\s*$/);
	await expect(fitting.filter({ hasText: 'Landeplatz Jägerkamp/Nagelspitz' })).toBeVisible();
	await expect(list.getByText('Fluggelände Jägerkamp/Nagelspitz')).toHaveCount(0);
	await keep('145-sites-list-filter-open');

	// The browser remembers the filter, as it does the map's layers.
	await page.reload();
	await mapTabDrawn(page, mapDrawn);
	await expect(page.getByRole('button', { name: 'Filter sites', exact: true })).toBeVisible();
	await showTheValley(page, mapDrawn, wide ? 1 : 2);
	await expect(
		page.getByRole('button', { name: /^Landings\s*[1-9]\d* landings\s*$/ })
	).toBeVisible();
	await expect(shown(page)).toHaveText('Flying sites on this part of the map: 2.');

	expect(problems).toEqual([]);
});

test('a landing field mapped as an area is drawn as its outline, under its sign', async ({
	context,
	page,
	mapDrawn,
	keep,
	problems
}) => {
	// The pilot stands at the lower edge of the landing field of the
	// Jägerkamp, which OpenStreetMap maps as an area of eight points.
	await context.grantPermissions(['geolocation']);
	await context.setGeolocation({ latitude: 47.6916, longitude: 11.8874 });
	await page.goto('/');
	await mapTabDrawn(page, mapDrawn);
	await showMyPosition(page, mapDrawn);
	await expect(shown(page)).toHaveText(/^Flying sites on this part of the map: [1-9]\d*\.$/);
	// The field and the take-off above it, which is an area of five points;
	// the ramps and the other landing are points and have no outline.
	await expect
		.poll(async () => (await drawn(page)).areas)
		.toEqual(expect.arrayContaining([JAEGERKAMP_LANDING, JAEGERKAMP_TAKEOFF]));
	expect((await drawn(page)).areas).not.toContain('osm-free-flying:node/9908030941');

	// Closer, where a field is larger than its sign, the outline shows.
	const map = page.locator('canvas.maplibregl-canvas');
	await map.focus();
	for (let step = 0; step < 3; step += 1) {
		await page.keyboard.press('Equal');
		await mapDrawn();
	}
	await map.blur();
	await page.waitForTimeout(1500);
	await keep('146-site-outline');

	expect(problems).toEqual([]);
});

test('a site’s page shows its mapper’s description as text', async ({ page, keep, problems }) => {
	// One of the Wallberg's two ramps, which OpenStreetMap describes.
	await page.goto(
		`/logbook/everyone/place/?lat=47.66522&lon=11.79542&site=${encodeURIComponent('osm-free-flying:node/5964676196')}`
	);
	const more = page.getByRole('region', { name: 'What its source says about it' });
	await expect(more).toHaveText('Startrampe (vermutlich für Drachenflieger)');
	// Nothing is said of a fee, of guests or of a time without flying, and it has no web page.
	await expect(more.getByRole('link')).toHaveCount(0);
	await expect(page.getByRole('button', { name: 'Keep as my take-off' })).toBeEnabled();
	await keep('131-site-description');
	expect(problems).toEqual([]);
});
