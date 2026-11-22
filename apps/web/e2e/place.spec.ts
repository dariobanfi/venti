/**
 * A place pilots fly from, from the Venti server in its development mode:
 * its flying sites are what OpenStreetMap had for the northern Alps on
 * 18 November 2026, and five of its public flights take off beside the
 * two ramps on the Wallberg, the others at the made-up sites near Garmisch. `playwright.config.ts` starts the server.
 */

import type { Page } from '@playwright/test';
import {
	expect,
	flightDetails,
	mapTabDrawn,
	showTheValley,
	rowsOf,
	test,
	BETWEEN_THE_MOUNTAINS,
	mapTool
} from './helpers';

/** The first of the Wallberg's two ramps, where the made-up flights take off. */
const RAMP = 'osm-free-flying:node/5964676196';
const WALLBERG = `/logbook/everyone/place/?lat=47.66522&lon=11.79542&site=${encodeURIComponent(RAMP)}`;

// Exactly: the site's own "The best flights from here" is another list.
const flights = (page: Page) => rowsOf(page, /^Flights from here$/);
const ranked = (page: Page) =>
	page.getByRole('list', { name: /^Ranking from here/ }).locator('> li');

test('a site’s page lists what was flown from it, the best first, and ranks its pilots', async ({
	page,
	keep,
	problems
}) => {
	await page.goto(WALLBERG);

	// An unnamed ramp is said by what it is.
	await expect(page.getByRole('heading', { level: 1, name: 'Flown from here' })).toBeVisible();
	await expect(page.locator('header').first()).toContainText('Take-off');
	await expect(page).toHaveTitle('Flown from here · Venti');
	// Every public flight of the development mode took off here.
	await expect(flights(page)).toHaveCount(5);
	await expect(page.getByRole('radio', { name: 'Newest' })).toBeChecked();
	await expect(
		page.getByText(/took off within a kilometre and a half of this place/).first()
	).toBeVisible();

	// The site, as its source has it, with its source's credit beside it.
	const site = page.getByRole('region', { name: 'The site' });
	await expect(site.getByRole('link', { name: 'See or correct it at the source' })).toHaveAttribute(
		'href',
		'https://www.openstreetmap.org/node/5964676196'
	);
	await site.getByRole('group', { name: 'How this is read' }).locator('summary').click();
	await expect(site.getByRole('link', { name: '© OpenStreetMap contributors' })).toBeVisible();
	await expect(site).toContainText('The flights are found by where they took off, not by the site');
	await keep('86-place');

	// The records of the place: the best first, by points.
	await page.getByRole('radio', { name: 'Best' }).check();
	await expect(page).toHaveURL(/order=best/);
	// Three flights share the most points; the first of them leads.
	await expect(flights(page).first()).toContainText('11.98');

	// The pilots of all years, ranked by what they flew from here.
	await page.getByRole('radio', { name: 'All years' }).check();
	await expect(page).toHaveURL(/season=All(%20|\+)years/);
	await expect(ranked(page)).toHaveCount(3);
	await expect(ranked(page).first()).toContainText('Mara Muster');
	await expect(
		page.getByText(/Only flights that took off within a kilometre and a half/)
	).toBeVisible();
	await keep('87-place-best');

	// The address holds both: opened again, the page is the same.
	await page.reload();
	await expect(page.getByRole('radio', { name: 'Best' })).toBeChecked();
	await expect(page.getByRole('radio', { name: 'All years' })).toBeChecked();
	await expect(ranked(page)).toHaveCount(3);

	// A flight opens like any other pilot's.
	await flights(page).first().getByRole('link').filter({ hasNotText: '°' }).last().click();
	await expect(page).toHaveURL(/\/logbook\/everyone\/flight\/\?id=/);
	expect(problems).toEqual([]);
});

test('a flight leads to the flights from its take-off', async ({ page, wide }) => {
	await page.goto('/logbook/everyone/');
	await rowsOf(page, 'Everyone’s flights')
		.first()
		.getByRole('link')
		.filter({ hasNotText: '°' })
		.last()
		.click();
	await expect(page).toHaveURL(/\/logbook\/everyone\/flight\/\?id=/);

	if (!wide) await page.getByRole('button', { name: 'Flight details' }).click();
	await flightDetails(page).getByRole('link', { name: 'Flights from this take-off' }).click();

	// The nearest take-off the source knows is the place's site.
	await expect(page).toHaveURL(/\/logbook\/everyone\/place\/\?lat=/);
	await expect(page.locator('header').first()).toContainText('Take-off');
	await expect(page.getByRole('region', { name: 'The site' })).toBeVisible();
	await expect(flights(page)).toHaveCount(5);
});

test('a site on the map leads to its page, which says when nobody flew from it', async ({
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
	await site.getByRole('link', { name: 'Flights from here' }).click();

	await expect(
		page.getByRole('heading', { level: 1, name: 'Fluggelände Jägerkamp/Nagelspitz' })
	).toBeVisible();
	await expect(page.locator('header').first()).toContainText('Take-off · 1560 m');
	await expect(page.getByText('Nobody shares a flight from here')).toBeVisible();
	await expect(page.getByText('Nobody is ranked')).toBeVisible();
	await keep('88-place-nobody');

	// A landing has no flights from it, and its page on the map no such link.
	await page.goBack();
	await mapTabDrawn(page, mapDrawn);
	await showTheValley(page, mapDrawn, wide ? 1 : 2);
	await mapTool(page, 'Flying sites');
	await list.getByRole('searchbox', { name: 'Site' }).fill('Landeplatz Jägerkamp');
	await list.getByRole('listitem').first().getByRole('button').click();
	const landing = page.getByRole(wide ? 'complementary' : 'dialog', {
		name: 'Landeplatz Jägerkamp/Nagelspitz'
	});
	await expect(landing.getByRole('button', { name: 'Forecast for this place' })).toBeVisible();
	await expect(landing.getByRole('link', { name: 'Flights from here' })).toHaveCount(0);
});

test('without the site, the flights from a place are listed all the same', async ({
	context,
	page
}) => {
	await context.route('**/v1/sites**', (route) => route.fulfill({ status: 503, body: '' }));
	await page.goto(WALLBERG);

	await expect(page.getByRole('heading', { level: 1, name: 'Flown from here' })).toBeVisible();
	await expect(page.locator('header').first()).toContainText(/server/i);
	await expect(page.getByRole('region', { name: 'The site' })).toHaveCount(0);
	await expect(flights(page)).toHaveCount(5);
});

test('an address without a place says so', async ({ page }) => {
	await page.goto('/logbook/everyone/place/?lat=north');
	await expect(page.getByRole('heading', { name: 'This address names no place' })).toBeVisible();
});
