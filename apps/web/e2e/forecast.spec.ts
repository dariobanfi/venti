/**
 * The forecast for a place, from the Venti server in its development mode:
 * what ICON-D2 and ICON-EU computed on 19 November 2026 for five places
 * around the Tegernsee, recorded at 19:30 UTC. `playwright.config.ts`
 * starts the server.
 */

import type { Locator, Page } from '@playwright/test';
import { BETWEEN_THE_MOUNTAINS, expect, mapTabDrawn, showTheValley, test } from './helpers';

/**
 * The evening the forecasts were recorded, so that "today" and the names
 * of the days are the same whenever the tests run.
 */
const THAT_EVENING = new Date('2026-11-19T20:00:00Z');

const forecastOf = (page: Page) => page.getByRole('dialog', { name: 'Forecast' });
const table = (forecast: Locator) =>
	forecast.getByRole('region', { name: 'The forecast, hour by hour' });

/** A chosen site or station: a sheet on a narrow screen, a card beside the map on a wide one. */
const panel = (page: Page, wide: boolean, name: string) =>
	page.getByRole(wide ? 'complementary' : 'dialog', { name });

/** Opens the page of the take-off on the Jägerkamp, from the list of sites. */
async function openTheTakeOff(
	page: Page,
	mapDrawn: () => Promise<void>,
	wide: boolean
): Promise<Locator> {
	await page.goto('/');
	await mapTabDrawn(page, mapDrawn);
	await showTheValley(page, mapDrawn, wide ? 1 : 2);
	await page.getByRole('button', { name: 'Flying sites' }).click();
	const list = page.getByRole('dialog', { name: 'Flying sites' });
	await list.getByRole('searchbox', { name: 'Site' }).fill('Fluggelände');
	await list.getByRole('listitem').first().getByRole('button').click();
	return panel(page, wide, 'Fluggelände Jägerkamp/Nagelspitz');
}

test('a site’s page leads to the forecast for its place, a day and a model at a time', async ({
	context,
	page,
	mapDrawn,
	keep,
	wide,
	problems
}) => {
	await page.clock.setFixedTime(THAT_EVENING);
	await context.grantPermissions(['geolocation']);
	await context.setGeolocation(BETWEEN_THE_MOUNTAINS);
	const site = await openTheTakeOff(page, mapDrawn, wide);
	await site.getByRole('button', { name: 'Forecast for this place' }).click();

	// The finest model first, and the first day with the sun up: the
	// evening it was recorded has no daylight left.
	const forecast = forecastOf(page);
	await expect(forecast).toContainText('Fluggelände Jägerkamp/Nagelspitz');
	const days = forecast.getByRole('radiogroup', { name: 'Day' });
	await expect(days.getByRole('radio', { name: 'Fri 20' })).toBeChecked();
	await expect(days.getByRole('radio', { name: 'Sat 21' })).not.toBeChecked();

	// A column for every hour with the sun up, a row for the wind at each
	// height, the highest at the top, then the wind at the ground and the sky.
	const hours = table(forecast).getByRole('columnheader');
	await expect(hours.nth(1)).toHaveText('8');
	await expect(hours.last()).toHaveText('16');
	const rows = table(forecast).getByRole('rowheader');
	await expect(rows.first()).toContainText(/^\s*[\d,]+ m\s*500 hPa\s*$/);
	await expect(rows.filter({ hasText: 'above ground' })).toHaveText([/180 m/, /120 m/, /80 m/]);
	await expect(rows.filter({ hasText: 'with gusts' })).toContainText('10 m');
	for (const name of ['Cloud', 'Rain', 'Temperature', '0 °C']) {
		await expect(rows.filter({ hasText: new RegExp(`^\\s*${name}\\s`) }).first()).toBeVisible();
	}
	// A wind is an arrow with its speed, and for a screen reader a sentence.
	const ground = table(forecast).getByRole('row', { name: /^10 m with gusts/ });
	await expect(ground.getByRole('cell').first()).toContainText(/\d+(\/\d+)?/);
	await expect(ground.getByRole('cell').first()).toContainText(/\d+( gusting \d+)? from [NESW]+/);

	// Under the table: what the numbers are, and what they are not.
	await expect(forecast).toContainText(
		/What the weather model ICON-D2 computes for the cell of its grid that holds this place \(2 km cells\)\. The model's ground there is at [\d,]+ m: its wind at 10 m is the wind 10 m above that/
	);
	await expect(forecast).toContainText('Nothing here says whether the day can be flown.');
	// The run, the model's ground, and whose data it is.
	await expect(forecast).toContainText('Thu, 19 Nov 2026, 19:00');
	await expect(forecast).toContainText('The model’s ground here');
	await expect(
		forecast.getByRole('link', { name: /^Weather data by Open-Meteo\.com/ })
	).toHaveAttribute('href', 'https://open-meteo.com/');
	await keep('49-forecast');

	// Another day of the same model.
	await days.getByText('Sat 21').click();
	await expect(days.getByRole('radio', { name: 'Sat 21' })).toBeChecked();
	await expect(hours.nth(1)).toHaveText('8');

	// Another model: its own run, its own ground, its own reach. ICON-EU's
	// run of that afternoon ends the next evening, so there is one day.
	await forecast.getByRole('combobox', { name: 'Weather model' }).selectOption('ICON-EU');
	await expect(forecast).toContainText(/What the weather model ICON-EU computes .* \(7 km cells\)/);
	await expect(forecast).toContainText('Thu, 19 Nov 2026, 16:00');
	await expect(days).toBeHidden();
	await table(forecast).scrollIntoViewIfNeeded();
	await keep('50-forecast-other-model');

	// Closed, the site's page is where it was.
	await forecast.getByRole('button', { name: 'Done' }).click();
	await expect(forecast).toBeHidden();
	await expect(site).toBeVisible();
	expect(problems).toEqual([]);
});

test('a wind station’s page leads to the forecast for its place too', async ({
	page,
	mapDrawn,
	wide
}) => {
	await page.clock.setFixedTime(THAT_EVENING);
	await page.goto('/');
	await mapTabDrawn(page, mapDrawn);
	await page.getByRole('button', { name: 'Wind stations' }).click();
	const list = page.getByRole('dialog', { name: 'Wind stations' });
	await list.getByRole('searchbox', { name: 'Station' }).fill('Zugspitze');
	await list.getByRole('listitem').first().getByRole('button').click();

	const station = panel(page, wide, 'Zugspitze');
	await station.getByRole('button', { name: 'Forecast for this place' }).click();
	const forecast = forecastOf(page);
	await expect(forecast).toContainText('Zugspitze');
	await expect(
		table(forecast).getByRole('rowheader').filter({ hasText: 'with gusts' })
	).toBeVisible();
	await expect(forecast).toContainText('Nothing here says whether the day can be flown.');
});

test('a forecast the server cannot give is said in its words, and asked for again on request', async ({
	context,
	page,
	mapDrawn,
	keep,
	wide
}) => {
	await page.clock.setFixedTime(THAT_EVENING);
	await context.route('**/v1/forecast**', (route) =>
		route.fulfill({
			status: 503,
			headers: { 'retry-after': '30' },
			json: { error: { code: 'unavailable', message: 'for developers' } }
		})
	);
	await context.grantPermissions(['geolocation']);
	await context.setGeolocation(BETWEEN_THE_MOUNTAINS);
	const site = await openTheTakeOff(page, mapDrawn, wide);
	await site.getByRole('button', { name: 'Forecast for this place' }).click();

	const forecast = forecastOf(page);
	await expect(forecast).toContainText('No forecast');
	await expect(forecast).toContainText(
		'The server is busy or being restarted. Try again in a moment.'
	);
	await keep('51-forecast-unavailable');

	await context.unroute('**/v1/forecast**');
	await forecast.getByRole('button', { name: 'Try again' }).click();
	await expect(table(forecast)).toBeVisible();
});
