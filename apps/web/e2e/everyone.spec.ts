/**
 * Everyone's flights, from the Venti server in its development mode: five
 * public flights of three made-up pilots, flown on 15 and 16 August and on
 * 6 September 2026. Every one is the sample flight, whole or cut short.
 * `playwright.config.ts` starts the server.
 */

import type { Page } from '@playwright/test';
import { expect, metric, panel, test } from './helpers';

/** The newest of the five, Jonas Beispiel's of 6 September. */
const NEWEST = 'nfmvgygyr34epnfbcekb04nwqc';

const flights = (page: Page) => page.getByRole('list', { name: 'Everyone’s flights' });
const rows = (page: Page) => flights(page).getByRole('listitem');
const whose = (page: Page) => page.getByRole('navigation', { name: 'Whose flights' });

test('everyone’s flights are listed newest first or best, on any day or on the days chosen', async ({
	page,
	keep,
	wide,
	problems
}) => {
	// The logbook has a second side where the site has a Venti server.
	await page.goto('/logbook/');
	await expect(whose(page).getByRole('link', { name: 'Mine' })).toHaveAttribute(
		'aria-current',
		'page'
	);
	await whose(page).getByRole('link', { name: 'Everyone' }).click();
	await expect(page).toHaveURL(/\/logbook\/everyone\/$/);
	await expect(page).toHaveTitle('Everyone’s flights · Venti');

	// The newest first: who flew, when and with what, how far and how long.
	await expect(rows(page)).toHaveCount(5);
	const first = rows(page).first();
	await expect(first.getByRole('link', { name: 'Jonas Beispiel' })).toBeVisible();
	await expect(first).toContainText(/Sun, 6 Sept? 2026 · Sample Wing/);
	await expect(first).toContainText('12.0 km');
	await expect(first).toContainText('0:29 h');
	await expect(rows(page).last()).toContainText('Demo Pilot');
	// Whose reckoning the numbers are is said under the list.
	await expect(page.getByText(/Distances and points are Venti’s own reckoning/)).toBeVisible();
	await keep('61-everyone');

	// The best by points. The address says what is asked for.
	await page.getByRole('radio', { name: 'Best' }).check();
	await expect(page).toHaveURL(/\?order=best$/);
	await expect(rows(page).last()).toContainText('9.0 km');
	// A wide screen has room for the points themselves.
	if (wide) await expect(rows(page).first()).toContainText('11.98');

	// One day: choosing the first day alone means that day.
	await page.getByLabel('From').fill('2026-08-15');
	await expect(page.getByLabel('to', { exact: true })).toHaveValue('2026-08-15');
	await expect(rows(page)).toHaveCount(2);
	await expect(rows(page).first()).toContainText('Demo Pilot');
	// A span of days.
	await page.getByLabel('to', { exact: true }).fill('2026-08-16');
	await expect(rows(page)).toHaveCount(4);
	await expect(page).toHaveURL(/\?order=best&from=2026-08-15&to=2026-08-16$/);
	await expect(rows(page).first()).toContainText('Mara Muster');
	await expect(rows(page).first()).toContainText('Sun, 16 Aug 2026');
	await keep('62-everyone-best-days');

	// The address holds the question: opened again, the list is the same.
	await page.reload();
	await expect(page.getByRole('radio', { name: 'Best' })).toBeChecked();
	await expect(page.getByLabel('From')).toHaveValue('2026-08-15');
	await expect(rows(page)).toHaveCount(4);

	// Days on which nobody flew say so, and any day brings the list back.
	await page.getByRole('button', { name: 'Any day' }).click();
	await expect(rows(page)).toHaveCount(5);
	await page.getByLabel('From').fill('2026-07-01');
	await expect(
		page.getByRole('heading', { name: 'No public flights on these days' })
	).toBeVisible();
	await keep('63-everyone-no-flights');
	await page.getByRole('button', { name: 'Any day' }).click();
	await expect(rows(page)).toHaveCount(5);

	// The pilot's own side is one click away, and comes back to the same list.
	await whose(page).getByRole('link', { name: 'Mine' }).click();
	await expect(page.getByRole('heading', { name: 'No flights yet' })).toBeVisible();
	await whose(page).getByRole('link', { name: 'Everyone' }).click();
	await expect(page.getByRole('radio', { name: 'Best' })).toBeChecked();
	expect(problems).toEqual([]);
});

test('a long list is read a page at a time', async ({ context, page }) => {
	// The server's pages hold fifty flights and its development mode has
	// five: here it is asked for two at a time.
	await context.route('**/v1/flights/public**', (route) => {
		const address = new URL(route.request().url());
		address.searchParams.set('limit', '2');
		return route.continue({ url: address.href });
	});
	await page.goto('/logbook/everyone/');

	const more = page.getByRole('button', { name: 'More flights' });
	await expect(rows(page)).toHaveCount(2);
	await more.click();
	await expect(rows(page)).toHaveCount(4);
	await more.click();
	await expect(rows(page)).toHaveCount(5);
	// The last page has been read.
	await expect(more).toBeHidden();
	await expect(rows(page).last()).toContainText('Demo Pilot');
});

test('a pilot’s name leads to that pilot’s flights', async ({ page, keep }) => {
	await page.goto('/logbook/everyone/');
	await rows(page).first().getByRole('link', { name: 'Jonas Beispiel' }).click();
	await expect(page).toHaveURL(/\/logbook\/everyone\/pilot\/\?handle=jonas$/);

	await expect(page.getByRole('heading', { level: 1, name: 'Jonas Beispiel' })).toBeVisible();
	await expect(page.getByText('@jonas', { exact: true })).toBeVisible();
	const own = page.getByRole('list', { name: 'Flights of Jonas Beispiel' }).getByRole('listitem');
	await expect(own).toHaveCount(2);
	// The rows of one pilot's flights begin with the day.
	await expect(own.first().getByRole('link')).toHaveText(/Sun, 6 Sept? 2026\s*Sample Wing/);
	await expect(own.last()).toContainText('9.0 km');
	await expect(page.getByText(/^The flights this pilot shares with everyone/)).toBeVisible();
	await keep('64-pilot-flights');

	await page.getByRole('link', { name: 'Everyone', exact: true }).first().click();
	await expect(rows(page)).toHaveCount(5);
});

test('a handle the server knows no pilot by says so in the core’s words', async ({ page }) => {
	await page.goto('/logbook/everyone/pilot/?handle=nobody');

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
	await page.goto('/logbook/everyone/');
	// The whole row opens the flight.
	await rows(page)
		.first()
		.getByRole('link', { name: /Sun, 6 Sept? 2026/ })
		.click();
	await expect(page).toHaveURL(new RegExp(`/logbook/everyone/flight/\\?id=${NEWEST}$`));

	// The flight screen of the logbook, with the pilot's name over the day.
	await expect(page.getByRole('heading', { level: 1 })).toHaveText(/Sun, 6 Sept? 2026/);
	await expect(page).toHaveTitle(/^Jonas Beispiel · Sun, 6 Sept? 2026 · Venti$/);
	const pilot = page.getByRole('link', { name: 'Jonas Beispiel' });
	await expect(pilot).toHaveAttribute('href', /\/logbook\/everyone\/pilot\/?\?handle=jonas$/);
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
	await expect(rows(page)).toHaveCount(5);

	// Nothing of it is in the pilot's own logbook.
	await whose(page).getByRole('link', { name: 'Mine' }).click();
	await expect(page.getByRole('heading', { name: 'No flights yet' })).toBeVisible();
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
	await page.goto('/logbook/everyone/');

	await expect(rows(page)).toHaveCount(5);
	// The letters are on the page as letters.
	await expect(rows(page).first().getByRole('link').first()).toHaveText(written.slice(0, 39) + '…');
	await expect(rows(page).first()).toContainText('<script>document.title = 2</script>');
	// Nothing of them became an element, and nothing of them ran.
	await expect(flights(page).locator('img, b, script')).toHaveCount(0);
	await expect(page).toHaveTitle('Everyone’s flights · Venti');
});

test('a flight its pilot took back says so, in the core’s words', async ({ page, keep }) => {
	await page.goto('/logbook/everyone/flight/?id=00000000000000000000000000');

	await expect(
		page.getByRole('heading', { name: 'This flight could not be opened' })
	).toBeVisible();
	await expect(
		page.getByText('This flight is no longer public: its pilot took it back, or deleted it.')
	).toBeVisible();
	await keep('66-public-flight-gone');
	await page.getByRole('link', { name: 'Back to everyone’s flights' }).click();
	await expect(rows(page)).toHaveCount(5);
});

test('a site without a server has a logbook without a second side', async ({ context, page }) => {
	// What a web server answers that only hands out the app's files.
	await context.route('**/v1/**', (route) => route.fulfill({ status: 404, body: 'Not found' }));
	await page.goto('/logbook/');
	await expect(page.getByRole('heading', { name: 'No flights yet' })).toBeVisible();
	await expect(whose(page)).toBeHidden();

	// Whoever has the address is told why there is nothing.
	await page.goto('/logbook/everyone/');
	await expect(page.getByRole('heading', { name: 'Not connected to Venti' })).toBeVisible();
});
