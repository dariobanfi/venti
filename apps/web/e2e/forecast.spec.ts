/**
 * The forecast for a place, from the Venti server in its development mode:
 * what ICON-D2 and ICON-EU computed in their runs of 06:00 UTC on
 * 21 November 2026 for five places around the Tegernsee, recorded at
 * 09:25 UTC. `playwright.config.ts` starts the server.
 */

import type { Locator, Page } from '@playwright/test';
import { BETWEEN_THE_MOUNTAINS, expect, mapTabDrawn, showTheValley, test } from './helpers';
import { madeUpSummer } from './made-up-summer';

/**
 * The morning the forecasts were recorded, half an hour after they were,
 * so that "today" and the names of the days are the same whenever the
 * tests run.
 */
const THAT_MORNING = new Date('2026-11-21T09:55:00Z');

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
	await page.goto('/map/');
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
	await page.clock.setFixedTime(THAT_MORNING);
	await context.grantPermissions(['geolocation']);
	await context.setGeolocation(BETWEEN_THE_MOUNTAINS);
	const site = await openTheTakeOff(page, mapDrawn, wide);
	// The recorded days have nothing to climb in: the air through the day
	// is a picture of the wind at heights.
	const air = site.getByRole('img', { name: /^The air through the day: / });
	await air.scrollIntoViewIfNeeded();
	await expect(air.locator('.arrow').first()).toBeAttached();
	await keep('107-site-air-recorded');
	await site.getByRole('button', { name: 'Forecast for this place' }).click();

	// The finest model first, and the first day with the sun up: today.
	const forecast = forecastOf(page);
	await expect(forecast).toContainText('Fluggelände Jägerkamp/Nagelspitz');
	const days = forecast.getByRole('radiogroup', { name: 'Day' });
	await expect(days.getByRole('radio', { name: 'Today' })).toBeChecked();
	await expect(days.getByRole('radio', { name: 'Sun 22' })).not.toBeChecked();

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
	await expect(forecast).toContainText('Sat, 21 Nov 2026, 07:00');
	await expect(forecast).toContainText('The model’s ground here');
	await expect(
		forecast.getByRole('link', { name: /^Weather data by Open-Meteo\.com/ })
	).toHaveAttribute('href', 'https://open-meteo.com/');
	await keep('49-forecast');

	// Another day of the same model.
	await days.getByText('Sun 22').click();
	await expect(days.getByRole('radio', { name: 'Sun 22' })).toBeChecked();
	await expect(hours.nth(1)).toHaveText('8');

	// Another model: its own run, its own ground, its own reach. ICON-EU's
	// run reaches 78 hours, to noon of the 24th: four days, and the day
	// chosen stays chosen.
	await forecast.getByRole('combobox', { name: 'Weather model' }).selectOption('ICON-EU');
	await expect(forecast).toContainText(/What the weather model ICON-EU computes .* \(7 km cells\)/);
	await expect(forecast).toContainText('Sat, 21 Nov 2026, 07:00');
	await expect(days.getByRole('radio')).toHaveCount(4);
	for (const name of ['Today', 'Sun 22', 'Mon 23', 'Tue 24']) {
		await expect(days.getByRole('radio', { name })).toBeAttached();
	}
	await expect(days.getByRole('radio', { name: 'Sun 22' })).toBeChecked();
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
	await page.clock.setFixedTime(THAT_MORNING);
	await page.goto('/map/');
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
	await page.clock.setFixedTime(THAT_MORNING);
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

test('a take-off’s page names the kind of day under the forecast’s line', async ({
	context,
	page,
	mapDrawn,
	keep,
	wide
}) => {
	// MADE UP: no model forecast this. See `made-up-summer.ts`.
	await context.route('**/v1/forecast**', madeUpSummer);
	await context.grantPermissions(['geolocation']);
	await context.setGeolocation(BETWEEN_THE_MOUNTAINS);
	const site = await openTheTakeOff(page, mapDrawn, wide);
	await expect(
		site.getByText(/^(Big day|Cross-country day|Local thermals|Weak thermals)$/)
	).toBeVisible({
		timeout: 30_000
	});
	await expect(site.getByText(/^(\d\.\d m\/s to .*|None to climb in)$/)).toBeVisible();
	await keep('106-site-thermals');
	// The air through the day: a column for the thermals of each hour, and
	// a cloud on it where cumulus caps them.
	const air = site.getByRole('img', { name: /^The air through the day: \d\.\d m\/s/ });
	await air.scrollIntoViewIfNeeded();
	await expect(air.locator('.column').first()).toBeAttached();
	await expect(air.locator('.cloud').first()).toBeAttached();
	await expect(air.getByText('Take-off')).toBeAttached();
	await keep('108-site-air-thermals');

	// The forecast's table has the estimate as two rows of its own, and says what they are.
	await site.getByRole('button', { name: 'Forecast for this place' }).click();
	const forecast = forecastOf(page);
	const names = table(forecast).getByRole('rowheader');
	await expect(names.filter({ hasText: 'Thermals, climb' })).toBeVisible();
	await expect(names.filter({ hasText: 'Thermals, top' })).toBeVisible();
	await expect(table(forecast).getByText('base', { exact: true }).first()).toBeAttached();
	await expect(forecast).toContainText('not a value of the model');
	await table(forecast).scrollIntoViewIfNeeded();
	await keep('109-forecast-thermals');
});

test('a take-off’s page says what is much stronger near by, and marks the layers in the air through the day', async ({
	context,
	page,
	mapDrawn,
	keep,
	wide,
	problems
}) => {
	// The development server's clock stands still on the 18th, and a
	// take-off's page is about the first day with flying hours left. The
	// forecast is the server's own recorded one; only the moment it is read
	// at is moved, to the evening before the day the test wants.
	let now = '2026-11-22T17:00:00Z';
	await context.route('**/v1/health', async (route) => {
		const response = await route.fetch();
		await route.fulfill({ response, json: { ...(await response.json()), time: now } });
	});
	await context.grantPermissions(['geolocation']);
	await context.setGeolocation(BETWEEN_THE_MOUNTAINS);

	// Monday: in one hour the gusts near by are much stronger than at the
	// take-off. The day says so under its line, as a note and not a status.
	let site = await openTheTakeOff(page, mapDrawn, wide);
	const line = site.getByTestId('nearby-line').first();
	await expect(line).toHaveText(
		/^Much stronger near by at \d+:00: .+ \d+ km\/h .+ \d+ km\/h at the place$/,
		{
			timeout: 30_000
		}
	);
	await expect(site.locator('.status')).toHaveText(/^\s*(Flyable|Marginal|Not flyable)\s*$/);
	await keep('147-site-nearby');

	// The picture marks where thermals stop, and says what the marks are.
	let air = site.getByRole('img', { name: /^The air through the day: / });
	await air.scrollIntoViewIfNeeded();
	await expect(air.locator('.layer').first()).toBeAttached();
	await expect(site).toContainText(
		'A hatched band is a stable layer and a solid band an inversion.'
	);
	await expect(site).toContainText(/Layers are read between the model.s levels/);
	await expect(site).toContainText(/Near by is the strongest wind and gust/);

	// The hour with the stronger wind, chosen: its line, its layers in the
	// core's words, and the strongest wind and gust near by with where.
	const hour = site.getByRole('button', { name: /^\d+:00: .+ km\/h at the place\. / });
	await hour.click();
	await expect(hour).toHaveAttribute('aria-pressed', 'true');
	const detail = site.getByTestId('air-hour');
	await expect(detail.getByTestId('nearby-line')).toHaveText(
		/ \d+ km\/h .+ \d+ km\/h at the place$/
	);
	await expect(detail.getByRole('listitem').first()).toHaveText(
		/^\s*(Stable layer|Inversion) between \d+ and \d+ m: \d\.\d °C (warmer )?per 100 m\s*$/
	);
	const where =
		'(in the square around the place|\\d+ km to the [a-z-]+)( over ground at [\\d,]+ m)?';
	await expect(detail).toContainText(new RegExp(`Wind near by\\s*\\d+ km/h ${where}`));
	await expect(detail).toContainText(new RegExp(`Gusts near by\\s*\\d+ km/h ${where}`));
	await detail.scrollIntoViewIfNeeded();
	await keep('148-site-air-hour');

	// Sunday: a quiet day near by, with an inversion over the ground in the
	// morning and a stable layer above it. No line, and both kinds of band.
	now = '2026-11-21T17:00:00Z';
	site = await openTheTakeOff(page, mapDrawn, wide);
	air = site.getByRole('img', { name: /^The air through the day: / });
	await air.scrollIntoViewIfNeeded();
	await expect(air.locator('.layer.inversion').first()).toBeAttached();
	await expect(air.locator('.layer:not(.inversion)').first()).toBeAttached();
	await expect(site.getByTestId('nearby-line')).toHaveCount(0);
	await keep('149-site-air-layers');
	await site
		.getByRole('button', { name: /^\d+:00: Inversion between / })
		.first()
		.click();
	await expect(site.getByTestId('air-hour')).toContainText(
		/Inversion between \d+ and \d+ m: \d\.\d °C warmer per 100 m/
	);
	expect(problems).toEqual([]);
});
