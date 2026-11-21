/**
 * The map's "Forecast" layer: the kind of day, square by square, from the
 * server's thermal map.
 *
 * The development server's map is a recorded November day of four squares
 * near the Tegernsee, nothing to climb in. Every kind of day is seen with
 * the made-up map of `tools/made-up-day/serve.py`, which
 * `playwright.config.ts` starts in front of the development server: the
 * first test asks it for the map in place of the development server. Its
 * map is made up, and so is everything its screenshots show.
 */

import type { Page } from '@playwright/test';
import { BETWEEN_THE_MOUNTAINS, expect, mapTabDrawn, showTheValley, test } from './helpers';

const MADE_UP_DAY = process.env.VENTI_MADE_UP_DAY_SERVER ?? 'http://127.0.0.1:8081';

/** Answers the thermal map with the stand-in's made-up one. */
async function madeUpMap(page: Page) {
	await page.route('**/v1/thermals', async (route) => {
		const response = await route.fetch({ url: `${MADE_UP_DAY}/v1/thermals` });
		await route.fulfill({ response });
	});
}

const forecastChip = (page: Page) =>
	page.getByRole('group', { name: 'Layers on the map' }).getByRole('button', { name: 'Forecast' });

/** The card of the layer: a pane beside the map on a wide screen, a card at its foot on a phone. */
const card = (page: Page) => page.getByRole('region', { name: 'Forecast map' });

test('the forecast layer shows every kind of day, day by day, and what a square says', async ({
	context,
	page,
	mapDrawn,
	keep,
	wide
}) => {
	await madeUpMap(page);
	// MADE UP: the stand-in's forecast too, which has the same three days
	// as its map, by the development server's clock; the recorded
	// forecast's days are others.
	await page.route('**/v1/forecast**', async (route) => {
		const asked = new URL(route.request().url());
		const response = await route.fetch({ url: `${MADE_UP_DAY}${asked.pathname}${asked.search}` });
		await route.fulfill({ response });
	});
	await context.grantPermissions(['geolocation']);
	await context.setGeolocation(BETWEEN_THE_MOUNTAINS);
	await page.goto('/');
	await mapTabDrawn(page, mapDrawn);

	// Off until the pilot switches it on.
	await expect(forecastChip(page)).toHaveAttribute('aria-pressed', 'false');
	await expect(card(page)).toBeHidden();
	await forecastChip(page).click();
	await expect(forecastChip(page)).toHaveAttribute('aria-pressed', 'true');

	// The key is the core's: the kinds of day from the best down, then the
	// day that is not flyable.
	const key = card(page).getByRole('list', { name: 'Key' });
	await expect(key.getByRole('listitem')).toHaveText([
		'Big day',
		'Cross-country day',
		'Local thermals',
		'Weak thermals',
		'Not flyable'
	]);
	await expect(card(page)).toContainText(/ICON-D2, run of \w{3}, \d+ \w{3} at \d\d:\d\d/);
	// A square cannot be reached with the keys; a take-off can.
	await expect(card(page).getByRole('link', { name: 'Today' })).toBeVisible();
	await expect(page.getByText(/^Forecast: Open-Meteo/)).toBeVisible();
	const days = card(page).getByRole('radiogroup', { name: 'Day' }).getByRole('radio');
	await expect(days).toHaveCount(3);
	await expect(days.first()).toBeChecked();
	await mapDrawn();
	await keep('112-map-forecast');

	// The next day, a step weaker everywhere. Days are chosen by their place:
	// which one is today hangs on the browser's time zone too.
	await card(page).getByRole('radiogroup', { name: 'Day' }).locator('label').nth(1).click();
	await expect(days.nth(1)).toBeChecked();
	await mapDrawn();
	await keep('113-map-forecast-tomorrow');

	// A square, chosen where nothing else is drawn: the other layers off.
	for (const name of ['Wind', 'Sites', 'Pilots']) {
		await page
			.getByRole('group', { name: 'Layers on the map' })
			.getByRole('button', { name })
			.click();
	}
	await showTheValley(page, mapDrawn, 1);
	const canvas = page.locator('canvas.maplibregl-canvas');
	const box = (await canvas.boundingBox())!;
	// Over the valley, above the card that stands at the foot of a phone's map.
	await canvas.click({ position: { x: box.width / 2, y: box.height * (wide ? 0.5 : 0.3) } });
	const square = wide ? card(page) : page.getByRole('dialog');
	await expect(square).toContainText(/Judged at the model’s ground, \d+ m/);
	await expect(square).toContainText(/Flyable|Marginal|Not flyable/);
	await expect(square).toContainText(/m\/s|None to climb in|Not flyable/);
	await mapDrawn();
	await keep('114-map-forecast-square');

	// The hours behind the day's verdict: the forecast for the square's middle,
	// open on the day the map shows, the second, of the model the map read it
	// from. The card shows all three of the stand-in's days, so its second is
	// the second of the stand-in's answer.
	const shown = (await (await fetch(`${MADE_UP_DAY}/v1/thermals`)).json()).days[1];
	await square.getByRole('button', { name: 'Forecast for this place' }).click();
	const forecast = page.getByRole('dialog', { name: 'Forecast' });
	await expect(forecast).toContainText(/\d+\.\d{4}° N, \d+\.\d{4}° E/);
	await expect(forecast.getByRole('table').first()).toBeVisible();
	const browsersToday = await page.evaluate(() =>
		new Date().toLocaleDateString('en-CA', { timeZone: 'Europe/Berlin' })
	);
	// As the forecast names a day: "Today", else "Sun 22".
	const weekdayAndDay = { weekday: 'short', day: 'numeric', timeZone: 'UTC' } as const;
	const noon = new Date(`${shown.date}T12:00:00Z`);
	const named =
		shown.date === browsersToday ? 'Today' : noon.toLocaleDateString('en-GB', weekdayAndDay);
	await expect(
		forecast.getByRole('radiogroup', { name: 'Day' }).getByRole('radio', { checked: true })
	).toHaveAccessibleName(named);
	// The map's second day is ICON-D2's, the one model of the stand-in's forecast.
	expect(shown.model).toBe('icon-d2');
	await expect(forecast).toContainText('What the weather model ICON-D2 computes');
	await keep('115-map-forecast-square-forecast');
	await forecast.getByRole('button', { name: 'Done' }).click();
	await expect(forecast).toBeHidden();

	// Switched off, the squares and the card go, and the choice is kept.
	if (!wide) await page.getByRole('dialog').getByRole('button', { name: 'Done' }).click();
	await forecastChip(page).click();
	await expect(card(page)).toBeHidden();
	await page.reload();
	await mapTabDrawn(page, mapDrawn);
	await expect(forecastChip(page)).toHaveAttribute('aria-pressed', 'false');
});

test('the forecast layer reads the development server’s November map, and says when there is none', async ({
	context,
	page,
	mapDrawn,
	wide
}) => {
	await context.grantPermissions(['geolocation']);
	await context.setGeolocation(BETWEEN_THE_MOUNTAINS);
	// First the server has no map: the card says so in the core's words.
	let refusing = true;
	await page.route('**/v1/thermals', async (route) => {
		if (refusing) await route.fulfill({ status: 503, body: '' });
		else await route.fallback();
	});
	await page.goto('/');
	await mapTabDrawn(page, mapDrawn);
	await forecastChip(page).click();
	await expect(card(page).getByRole('button', { name: 'Try again' })).toBeVisible();
	await expect(card(page).getByRole('list', { name: 'Key' })).toBeHidden();

	// Asked again, it has the recorded map: a day not flyable, then weak thermals.
	refusing = false;
	await card(page).getByRole('button', { name: 'Try again' }).click();
	await expect(card(page).getByRole('list', { name: 'Key' })).toBeVisible();
	await expect(card(page)).toContainText('ICON-D2, run of Sat, 21 Nov at 01:00');
	const days = card(page).getByRole('radiogroup', { name: 'Day' });
	await expect(days.getByRole('radio')).toHaveCount(2);
	await expect(days).toContainText('Sat');
	await expect(days).toContainText('Sun');

	// A square of the first day, around the pilot: not flyable, for rain.
	for (const name of ['Wind', 'Sites', 'Pilots']) {
		await page
			.getByRole('group', { name: 'Layers on the map' })
			.getByRole('button', { name })
			.click();
	}
	await showTheValley(page, mapDrawn, 1);
	const canvas = page.locator('canvas.maplibregl-canvas');
	const box = (await canvas.boundingBox())!;
	const below = wide
		? 0
		: await page
				.locator('nav')
				.first()
				.evaluate((nav) => nav.clientHeight);
	await canvas.click({ position: { x: box.width / 2, y: (box.height - below) / 2 - 10 } });
	const square = wide ? card(page) : page.getByRole('dialog');
	await expect(square).toContainText('Not flyable');
	await expect(square).toContainText('Rain');
	await expect(square).toContainText(/Judged at the model’s ground, \d+ m/);
	if (!wide) await page.getByRole('dialog').getByRole('button', { name: 'Done' }).click();
	for (const name of ['Wind', 'Sites', 'Pilots']) {
		await page
			.getByRole('group', { name: 'Layers on the map' })
			.getByRole('button', { name })
			.click();
	}

	// Kept on as the pilot left it.
	await page.reload();
	await mapTabDrawn(page, mapDrawn);
	await expect(forecastChip(page)).toHaveAttribute('aria-pressed', 'true');
	await expect(card(page).getByRole('list', { name: 'Key' })).toBeVisible();
});
