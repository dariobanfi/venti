/**
 * Everyone's flights, from the Venti server in its development mode: the
 * public flights of its made-up pilots, the newest Jonas Beispiel's of
 * 6 September 2026 and the oldest Timo Entwurf's of July 2025. Five of them
 * are the sample flight, whole or cut short, beside the Wallberg; the rest
 * take off at the made-up sites near Garmisch. `playwright.config.ts`
 * starts the server.
 */

import type { Page } from '@playwright/test';
import { expect, metric, panel, rowsOf, test } from './helpers';

/** The newest, Jonas Beispiel's of 6 September. */
const NEWEST = 'nfmvgygyr34epnfbcekb04nwqc';

/** How many public flights the development mode holds: 11 since 2026-11-21. */
const ALL = 11;

const rows = (page: Page) => rowsOf(page, 'Everyone’s flights');
const whose = (page: Page) => page.getByRole('navigation', { name: 'Whose flights' });

test('everyone’s flights are listed newest first or best, on any day or on the days chosen', async ({
	page,
	keep,
	wide,
	problems
}) => {
	// The pilot's own flights have a second side where the site has a Venti
	// server: the logbook's old address leads to them.
	await page.goto('/logbook/');
	await expect(whose(page).getByRole('link', { name: 'Mine' })).toHaveAttribute(
		'aria-current',
		'page'
	);
	await whose(page).getByRole('link', { name: 'Everyone' }).click();
	await expect(page).toHaveURL(/\/flights\/$/);
	await expect(page).toHaveTitle('Everyone’s flights · Venti');

	// The newest first: who flew, when and with what, how far and how long.
	await expect(rows(page)).toHaveCount(ALL);
	const first = rows(page).first();
	await expect(first.getByRole('link', { name: 'Jonas Beispiel' })).toBeVisible();
	await expect(first).toContainText(/6 Sep 2026[\s\S]*Sample Wing/);
	// A flight's date is day, month and year, with no weekday (the owner, 2026-11-23).
	await expect(first).not.toContainText(/\b(Mon|Tue|Wed|Thu|Fri|Sat|Sun)\b/);
	await expect(first).toContainText('12.0 km');
	await expect(first).toContainText('0:29 h');
	// The oldest last.
	await expect(rows(page).last()).toContainText('Timo Entwurf');
	// Whose reckoning the numbers are is said under the list.
	await expect(page.getByText(/Distances and points are Venti’s own reckoning/)).toBeVisible();
	await keep('61-everyone');

	// The best by points. The address says what is asked for.
	await page.getByRole('radio', { name: 'Best' }).check();
	await expect(page).toHaveURL(/\?order=best$/);
	// The fewest points last: Lena Probe's short flight of 6 June.
	await expect(rows(page).last()).toContainText('Lena Probe');
	await expect(rows(page).last()).toContainText('4.7 km');
	// A wide screen has room for the points themselves.
	if (wide) {
		await expect(rows(page).first()).toContainText('12.01');
		await expect(rows(page).last()).toContainText('4.71');
	}

	// One day: choosing the first day alone means that day. The days are
	// behind "Filter".
	const filter = page.getByRole('button', { name: 'Filter', exact: true });
	await filter.click();
	await page.getByLabel('From').fill('2026-08-15');
	await expect(page.getByLabel('to', { exact: true })).toHaveValue('2026-08-15');
	await expect(rows(page)).toHaveCount(2);
	await expect(rows(page).first()).toContainText('Demo Pilot');
	// A span of days.
	await page.getByLabel('to', { exact: true }).fill('2026-08-16');
	await expect(rows(page)).toHaveCount(4);
	await expect(page).toHaveURL(/\?order=best&from=2026-08-15&to=2026-08-16$/);
	await expect(rows(page).first()).toContainText('Mara Muster');
	await expect(rows(page).first()).toContainText('16 Aug 2026');
	// The span chosen stands beside "Filter" as a chip.
	await expect(page.getByText('15 Aug 2026 – 16 Aug 2026')).toBeVisible();
	await keep('62-everyone-best-days');

	// The address holds the question: opened again, the list is the same.
	await page.reload();
	await expect(page.getByRole('radio', { name: 'Best' })).toBeChecked();
	await expect(filter).toHaveAttribute('aria-expanded', 'false');
	await expect(page.getByText('15 Aug 2026 – 16 Aug 2026')).toBeVisible();
	await filter.click();
	await expect(page.getByLabel('From')).toHaveValue('2026-08-15');
	await expect(rows(page)).toHaveCount(4);

	// Days on which nobody flew say so, and any day brings the list back.
	await page.getByRole('button', { name: 'Any day' }).click();
	await expect(rows(page)).toHaveCount(ALL);
	await page.getByLabel('From').fill('2026-07-01');
	await expect(
		page.getByRole('heading', { name: 'No public flights on these days' })
	).toBeVisible();
	await keep('63-everyone-no-flights');
	await page.getByRole('button', { name: 'Any day' }).click();
	await expect(rows(page)).toHaveCount(ALL);

	// The pilot's own side is one click away, by the switch at the head of
	// the page, and comes back to the same list. The switch is Mine and
	// Everyone alone: the ranking is a section of the bar.
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Flights');
	await expect(whose(page).getByRole('link')).toHaveText(['Mine', 'Everyone']);
	await page.evaluate(() => window.scrollTo(0, 0));
	await expect(whose(page)).toBeInViewport();
	await keep('61a-flights-mine-or-everyone');
	await whose(page).getByRole('link', { name: 'Mine' }).click();
	await expect(page.getByRole('heading', { name: 'Sign in to see your flights' })).toBeVisible();
	await whose(page).getByRole('link', { name: 'Everyone' }).click();
	await expect(page.getByRole('radio', { name: 'Best' })).toBeChecked();
	expect(problems).toEqual([]);
});

test('a long list is read a page at a time', async ({ context, page }) => {
	// The server's pages hold fifty flights and its development mode has
	// fewer: here it is asked for two at a time, until every one is read.
	await context.route('**/v1/flights/public**', (route) => {
		const address = new URL(route.request().url());
		address.searchParams.set('limit', '2');
		return route.continue({ url: address.href });
	});
	await page.goto('/flights/');

	const more = page.getByRole('button', { name: 'More flights' });
	await expect(rows(page)).toHaveCount(2);
	for (let read = 2; read < ALL; read += 2) {
		await more.click();
		await expect(rows(page)).toHaveCount(Math.min(read + 2, ALL));
	}
	// The last page has been read.
	await expect(more).toBeHidden();
	await expect(rows(page).last()).toContainText('Timo Entwurf');
});

test('a pilot’s name leads to that pilot’s flights', async ({ page, keep, wide }) => {
	await page.goto('/flights/');
	await rows(page).first().getByRole('link', { name: 'Jonas Beispiel' }).click();
	await expect(page).toHaveURL(/\/pilots\/jonas\/$/);

	await expect(page.getByRole('heading', { level: 1, name: 'Jonas Beispiel' })).toBeVisible();
	await expect(page.getByText('@jonas', { exact: true })).toBeVisible();
	const own = rowsOf(page, 'Flights of Jonas Beispiel');
	await expect(own).toHaveCount(2);
	// The rows of one pilot's flights begin with the day.
	await expect(own.first().getByRole('link').first()).toHaveText(
		wide ? /^6 Sep 2026$/ : /6 Sep 2026\s*Sample Wing/
	);
	await expect(own.first()).toContainText('Sample Wing');
	await expect(own.last()).toContainText('9.0 km');
	await expect(page.getByText(/^The flights this pilot shares with everyone/)).toBeVisible();
	await keep('64-pilot-flights');

	await page.getByRole('link', { name: 'Everyone', exact: true }).first().click();
	await expect(rows(page)).toHaveCount(ALL);
});

test('a handle the server knows no pilot by says so in the core’s words', async ({ page }) => {
	await page.goto('/pilots/nobody/');

	await expect(
		page.getByRole('heading', { name: 'The flights could not be listed' })
	).toBeVisible();
	await expect(page.getByText('The server knows no pilot by this handle.')).toBeVisible();
	await expect(page.getByRole('button', { name: 'Try again' })).toBeVisible();
});

test('another pilot’s flight opens like one’s own, from its file, and is kept nowhere', async ({
	context,
	page,
	mapDrawn,
	keep,
	problems
}) => {
	await page.goto('/flights/');
	// The whole row opens the flight.
	await rows(page)
		.first()
		.getByRole('link', { name: /6 Sep 2026/ })
		.click();
	await expect(page).toHaveURL(new RegExp(`/flights/${NEWEST}/$`));

	// The flight screen of the logbook, with the pilot's name over the day.
	await expect(page.getByRole('heading', { level: 1 })).toHaveText(/6 Sep 2026/);
	await expect(page).toHaveTitle(/^Jonas Beispiel · 6 Sep 2026 · Venti$/);
	const pilot = page.getByRole('link', { name: 'Jonas Beispiel' });
	await expect(pilot).toHaveAttribute('href', /\/pilots\/jonas\/$/);
	await expect(metric(panel(page).totals, 'Airtime')).toHaveText(/^0:29\sh$/);
	await expect(metric(panel(page).totals, 'Highest')).toHaveText(/^2117\sm$/);
	await mapDrawn();
	await keep('65-public-flight');

	// What can be done with it is to hand its page on. It is not the
	// pilot's to share, to download from here or to delete.
	const menu = page.getByRole('button', { name: 'More' }).filter({ visible: true });
	await menu.click();
	await expect(page.getByRole('menuitem', { name: 'Copy the link to its page' })).toBeVisible();
	await expect(page.getByRole('menuitem', { name: 'Delete flight' })).toBeHidden();
	const [serversPage] = await Promise.all([
		context.waitForEvent('page'),
		page.getByRole('menuitem', { name: 'Open its page on Venti' }).click()
	]);
	await expect(serversPage).toHaveURL(new RegExp(`/f/${NEWEST}$`));
	await serversPage.close();

	// Back to the list it was opened from.
	await page.getByRole('link', { name: 'Everyone', exact: true }).filter({ visible: true }).click();
	await expect(rows(page)).toHaveCount(ALL);

	// Nothing of it is among the pilot's own flights, which ask to sign in.
	await whose(page).getByRole('link', { name: 'Mine' }).click();
	await expect(page.getByRole('heading', { name: 'Sign in to see your flights' })).toBeVisible();
	expect(problems).toEqual([]);
});

test('what another pilot wrote is shown as text, never as markup', async ({ context, page }) => {
	const written = '<img src=x onerror="document.title=1"><b>M</b>';
	await context.route('**/v1/flights/public**', async (route) => {
		const response = await route.fetch();
		const answer = await response.json();
		for (const flight of answer.flights) {
			flight.pilot.display_name = written;
			flight.glider = '<script>document.title = 2</script>';
		}
		await route.fulfill({ response, json: answer });
	});
	await page.goto('/flights/');

	await expect(rows(page)).toHaveCount(ALL);
	// The letters are on the page as letters.
	await expect(rows(page).first().getByRole('link').first()).toHaveText(written.slice(0, 39) + '…');
	await expect(rows(page).first()).toContainText('<script>document.title = 2</script>');
	// Nothing of them became an element, and nothing of them ran.
	// (A pilot's picture is an image of the server's, at its own address.)
	await expect(rows(page).locator('img:not([src^="/v1/pictures/"]), b, script')).toHaveCount(0);
	await expect(page).toHaveTitle('Everyone’s flights · Venti');
});

test('a take-off’s country and its pilot’s are flags, and a glider’s class stands beside it', async ({
	context,
	page,
	keep
}) => {
	// Made up: the development mode's flights take off in Germany, where the
	// server has no outline of a country, and its pilots set no country or
	// class. Every flight is said to take off in Austria, from a pilot of
	// Germany, on a glider of EN B.
	await context.route('**/v1/flights/public**', async (route) => {
		const response = await route.fetch();
		const answer = await response.json();
		for (const flight of answer.flights) {
			flight.takeoff_country = 'AT';
			flight.pilot.country = 'DE';
			flight.glider_class = 'en_b';
		}
		await route.fulfill({ response, json: answer });
	});
	await page.goto('/flights/');

	const first = rows(page).first();
	await expect(first).toContainText('Jonas Beispiel');
	await expect(first.getByRole('img', { name: 'Germany' })).toBeVisible();
	await expect(first.getByRole('img', { name: 'Austria' })).toBeVisible();
	await expect(first).toContainText('EN B · Sample Wing');
	await keep('92-everyone-countries');
});

test('a flight its pilot took back says so, in the core’s words', async ({ page, keep }) => {
	await page.goto('/flights/00000000000000000000000000/');

	await expect(
		page.getByRole('heading', { name: 'This flight could not be opened' })
	).toBeVisible();
	await expect(
		page.getByText('This flight is no longer public: its pilot took it back, or deleted it.')
	).toBeVisible();
	await keep('66-public-flight-gone');
	await page.getByRole('link', { name: 'Back to everyone’s flights' }).click();
	await expect(rows(page)).toHaveCount(ALL);
});

test('a site without a server has no second side to the pilot’s flights', async ({
	context,
	page
}) => {
	// What a web server answers that only hands out the app's files.
	await context.route('**/v1/**', (route) => route.fulfill({ status: 404, body: 'Not found' }));
	await page.goto('/logbook/');
	await expect(page.getByRole('heading', { name: 'Sign in to see your flights' })).toBeVisible();
	await expect(whose(page)).toBeHidden();

	// Whoever has the address is told why there is nothing.
	await page.goto('/flights/');
	await expect(page.getByRole('heading', { name: 'Not connected to Venti' })).toBeVisible();
});

test('the old addresses of the flights, a pilot, a place and the ranking lead to the new ones', async ({
	page,
	keep
}) => {
	// Links of before 2026-11-24 are out in the wild: each leads on, its query along.
	await page.goto('/logbook/everyone/?order=best');
	await expect(page).toHaveURL(/\/\/[^/]+\/flights\/\?order=best$/);
	await page.goto(`/logbook/everyone/flight/?id=${NEWEST}`);
	await expect(page).toHaveURL(new RegExp(`//[^/]+/flights/${NEWEST}/$`));
	await expect(page).toHaveTitle(/^Jonas Beispiel · /);
	await page.goto('/logbook/everyone/pilot/?handle=jonas');
	await expect(page).toHaveURL(/\/\/[^/]+\/pilots\/jonas\/$/);
	await expect(page.getByRole('heading', { level: 1, name: 'Jonas Beispiel' })).toBeVisible();
	await page.goto(
		'/logbook/everyone/place/?lat=47.66522&lon=11.79542&site=osm-free-flying%3Anode%2F5964676196&order=best'
	);
	await expect(page).toHaveURL(
		/\/\/[^/]+\/places\/47\.66522,11\.79542\/\?order=best&site=osm-free-flying%3Anode%2F5964676196$/
	);
	await page.goto('/logbook/ranking/?season=2025');
	await expect(page).toHaveURL(/\/\/[^/]+\/ranking\/\?season=2025$/);

	// A flight's own address opens it, also loaded straight from the web server.
	await page.goto(`/flights/${NEWEST}/`);
	await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
	await expect(page).toHaveTitle(/^Jonas Beispiel · /);
	await keep('65a-flight-at-its-address');
});
