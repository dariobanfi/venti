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

/** Answers the thermal map, and its hours, with the stand-in's made-up ones. */
async function madeUpMap(page: Page) {
	await page.route('**/v1/thermals', async (route) => {
		const response = await route.fetch({ url: `${MADE_UP_DAY}/v1/thermals` });
		await route.fulfill({ response });
	});
	await page.route('**/v1/thermals/hours?*', async (route) => {
		const asked = new URL(route.request().url());
		const response = await route.fetch({ url: `${MADE_UP_DAY}${asked.pathname}${asked.search}` });
		await route.fulfill({ response });
	});
}

/** The made-up map's grid and ground, as its stand-in answers them. */
async function madeUpGrid(): Promise<{
	grid: { south: number; west: number; step_deg: number; rows: number; columns: number };
	ground_m: number[];
}> {
	return (await fetch(`${MADE_UP_DAY}/v1/thermals`)).json();
}

/**
 * The middle of the square of the made-up map nearest the valley whose
 * ground is above or below a height: the test's map has the valley's
 * tiles and none further out.
 */
async function squareWhereGround(above: boolean, heightM: number) {
	const { grid, ground_m } = await madeUpGrid();
	const middle = (cell: number) => ({
		latitude: grid.south + Math.floor(cell / grid.columns) * grid.step_deg,
		longitude: grid.west + (cell % grid.columns) * grid.step_deg
	});
	const away = (cell: number) =>
		Math.hypot(
			middle(cell).latitude - BETWEEN_THE_MOUNTAINS.latitude,
			middle(cell).longitude - BETWEEN_THE_MOUNTAINS.longitude
		);
	const cells = ground_m
		.map((ground, cell) => ({ ground, cell }))
		.filter(({ ground }) => (above ? ground > heightM : ground < heightM))
		.toSorted((one, other) => away(one.cell) - away(other.cell));
	return middle(cells[0].cell);
}

/** Goes to a place on the made-up map, with the forecast layer on and nothing else of Venti's. */
async function forecastAt(
	context: import('@playwright/test').BrowserContext,
	page: Page,
	mapDrawn: () => Promise<void>,
	place: { latitude: number; longitude: number }
) {
	await context.grantPermissions(['geolocation']);
	await context.setGeolocation(place);
	await page.goto('/');
	await mapTabDrawn(page, mapDrawn);
	for (const name of ['Wind', 'Sites', 'Pilots']) {
		await page
			.getByRole('group', { name: 'Layers on the map' })
			.getByRole('button', { name })
			.click();
	}
	await forecastChip(page).click();
	await showTheValley(page, mapDrawn, 1);
}

/**
 * Clicks the map in the middle, or on a phone just above the card that
 * stands at the map's foot, where the middle may lie under it.
 */
async function clickTheMiddle(page: Page, wide: boolean) {
	const canvas = page.locator('canvas.maplibregl-canvas');
	const box = (await canvas.boundingBox())!;
	let y = box.height / 2;
	if (!wide) {
		const top = (await card(page).boundingBox())!.y - box.y;
		y = Math.min(y, top - 24);
	}
	await canvas.click({ position: { x: box.width / 2, y } });
}

/** Zooms the map in or out by steps, with the keyboard, and waits for it to be drawn. */
async function zoom(page: Page, mapDrawn: () => Promise<void>, steps: number) {
	const canvas = page.locator('canvas.maplibregl-canvas');
	await canvas.focus();
	for (let step = 0; step < Math.abs(steps); step += 1) {
		await page.keyboard.press(steps > 0 ? 'Equal' : 'Minus');
		await mapDrawn();
	}
	await canvas.blur();
}

/**
 * Moves the map's view up or down with the arrow keys, a hundred pixels a
 * press, and waits for it to be drawn.
 */
async function pan(
	page: Page,
	mapDrawn: () => Promise<void>,
	key: 'ArrowUp' | 'ArrowDown',
	presses: number
) {
	const canvas = page.locator('canvas.maplibregl-canvas');
	await canvas.focus();
	for (let press = 0; press < presses; press += 1) {
		await page.keyboard.press(key);
		await mapDrawn();
	}
	await canvas.blur();
}

/** Steps the line of hours to an hour by its label, with the keyboard. */
async function toHour(page: Page, label: string) {
	const hours = card(page).getByRole('slider', { name: 'Hour' });
	await hours.focus();
	await page.keyboard.press('Home');
	for (let step = 0; step < 16; step += 1) {
		if ((await hours.getAttribute('aria-valuetext')) === label) break;
		await page.keyboard.press('ArrowRight');
	}
	await expect(hours).toHaveAttribute('aria-valuetext', label);
	await hours.blur();
}

/** The card's row of layers. */
const layerChip = (page: Page, name: string) =>
	card(page).getByRole('group', { name: 'Layer' }).getByRole('button', { name, exact: true });

const forecastChip = (page: Page) =>
	page.getByRole('group', { name: 'Layers on the map' }).getByRole('button', { name: 'Forecast' });

/** The card of the layer: a pane beside the map on a wide screen, a card at its foot on a phone. */
const card = (page: Page) => page.getByRole('region', { name: 'Forecast map' });

/** What folds the card's key, run and notes away on a phone, where the card lies over the map. */
const keyAndNotes = (page: Page) => card(page).getByRole('button', { name: 'Key and notes' });

/**
 * Unfolds the card's key, run and notes on a phone, which opens with them
 * folded away. A wide screen shows them beside the map and has nothing to unfold.
 */
async function unfoldTheKey(page: Page, wide: boolean) {
	if (wide) {
		await expect(keyAndNotes(page)).toBeHidden();
		return;
	}
	await expect(keyAndNotes(page)).toHaveAttribute('aria-expanded', 'false');
	await expect(card(page).getByRole('list', { name: 'Key' })).toBeHidden();
	await keyAndNotes(page).click();
	await expect(keyAndNotes(page)).toHaveAttribute('aria-expanded', 'true');
	// One tap, and the whole key stands in the card.
	await expect(card(page).getByRole('list', { name: 'Key' })).toBeInViewport({ ratio: 1 });
}

/** Folds them away again, as the card opens. */
async function foldTheKey(page: Page, wide: boolean) {
	if (wide) return;
	await keyAndNotes(page).click();
	await expect(keyAndNotes(page)).toHaveAttribute('aria-expanded', 'false');
	await expect(card(page).getByRole('list', { name: 'Key' })).toBeHidden();
}

test('the forecast layer shows every kind of day, day by day, and what a square says', async ({
	context,
	page,
	mapDrawn,
	keep,
	wide
}) => {
	await madeUpMap(page);
	// MADE UP: the stand-in's forecast too, which has the same days as its
	// map, by the development server's clock; the recorded
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
	const days = card(page).getByRole('radiogroup', { name: 'Day' }).getByRole('radio');
	await expect(days).toHaveCount(4);
	await expect(days.first()).toBeChecked();
	await mapDrawn();
	await keep('112-map-forecast');

	// The key is the core's: the kinds of day from the best down, then the
	// day that is not flyable. On a phone it is one tap away.
	await unfoldTheKey(page, wide);
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
	if (!wide) await keep('130-map-forecast-key-and-notes');
	await foldTheKey(page, wide);

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
	// The square's days side by side at the head, as the sheet of an hour has
	// them: the day the map shows is marked.
	const strip = square.getByRole('group', { name: 'This square, day by day' }).getByRole('button');
	await expect(strip).toHaveCount(4);
	await expect(strip.nth(1)).toHaveAttribute('aria-pressed', 'true');
	await mapDrawn();
	await keep('114-map-forecast-square');

	// Another day from the strip: the map shows it, and the square stays open
	// with that day's reading. Then back to the second day.
	await strip.nth(2).click();
	await expect(strip.nth(2)).toHaveAttribute('aria-pressed', 'true');
	await expect(days.nth(2)).toBeChecked();
	await expect(square).toContainText(/Judged at the model’s ground, \d+ m/);
	await expect(square).toContainText('ICON-EU');
	await strip.nth(1).click();
	await expect(strip.nth(1)).toHaveAttribute('aria-pressed', 'true');
	await expect(days.nth(1)).toBeChecked();
	await expect(square).toContainText('ICON-D2');

	// The hours behind the day's verdict: the forecast for the square's middle,
	// open on the day the map shows, the second, of the model the map read it
	// from. The card shows all four of the stand-in's days, so its second is
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
	await unfoldTheKey(page, wide);
	await expect(card(page).getByRole('list', { name: 'Key' })).toBeVisible();
	await expect(card(page)).toContainText('ICON-D2, run of Sat, 21 Nov at 07:00');
	const days = card(page).getByRole('radiogroup', { name: 'Day' });
	await expect(days.getByRole('radio')).toHaveCount(4);
	await expect(days).toContainText('Sat');
	await expect(days).toContainText('Sun');
	await expect(days).toContainText('Mon');
	await expect(days).toContainText('Tue');

	// A square of the first day, around the pilot: not flyable, for rain.
	for (const name of ['Wind', 'Sites', 'Pilots']) {
		await page
			.getByRole('group', { name: 'Layers on the map' })
			.getByRole('button', { name })
			.click();
	}
	await showTheValley(page, mapDrawn, 1);
	await clickTheMiddle(page, wide);
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

	// Kept on as the pilot left it. The key is folded away again on a phone:
	// that is kept while the page is open, not by the browser.
	await page.reload();
	await mapTabDrawn(page, mapDrawn);
	await expect(forecastChip(page)).toHaveAttribute('aria-pressed', 'true');
	await unfoldTheKey(page, wide);
	await expect(card(page).getByRole('list', { name: 'Key' })).toBeVisible();
});

test('the forecast layer shows each layer hour by hour, and what a square says in an hour', async ({
	context,
	page,
	mapDrawn,
	keep,
	wide
}) => {
	// MADE UP: the stand-in's map and its hours; see `tools/made-up-day/serve.py`.
	await madeUpMap(page);
	const low = await squareWhereGround(false, 1000);
	await forecastAt(context, page, mapDrawn, low);
	// Out to where a field of squares and their arrows is in view: a square
	// of the made-up map is a tenth of a degree, and a phone is narrow.
	const out = wide ? -2 : -3;
	await zoom(page, mapDrawn, out);
	// On a phone the card lies over the map's middle, where the pilot stands:
	// the view goes south, so that the field stands in what the card leaves free.
	if (!wide) await pan(page, mapDrawn, 'ArrowDown', 2);

	// The layers above the days; "Thermals" first, and on it the line of
	// hours opens at the day as a whole.
	const chips = card(page).getByRole('group', { name: 'Layer' }).getByRole('button');
	await expect(chips).toHaveText(['Thermals', 'Wind', 'Gusts', 'Cloud', 'Rain']);
	await expect(layerChip(page, 'Thermals')).toHaveAttribute('aria-pressed', 'true');
	const hours = card(page).getByRole('slider', { name: 'Hour' });
	await expect(hours).toHaveAttribute('aria-valuetext', 'Day');

	// One hour on: that hour's thermals, under the hour's key. A step of an
	// hour is measured from the key to the map drawn again.
	await hours.focus();
	const started = Date.now();
	await page.keyboard.press('ArrowRight');
	await expect(hours).toHaveAttribute('aria-valuetext', /^\d\d:00$/);
	await mapDrawn();
	console.log(`A step of an hour took ${Date.now() - started} ms to draw.`);
	// Midday, where the made-up day has the most to see.
	await toHour(page, '13:00');
	await mapDrawn();
	await keep('120-map-forecast-thermals-hour');

	// The wind at the ground: arrows, opened at the hour the core names.
	await layerChip(page, 'Wind').click();
	await expect(layerChip(page, 'Wind')).toHaveAttribute('aria-pressed', 'true');
	await expect(hours).toHaveAttribute('aria-valuetext', /^\d\d:00$/);
	const height = card(page).getByRole('combobox', { name: 'Height' });
	await expect(height).toHaveValue('');
	await toHour(page, '13:00');
	// The layer and the hour in words, and the height's menu at their side
	// on the same line, clear of the row of layers.
	const words = card(page).getByText('Wind at the ground, 13:00');
	const wordsBox = (await words.boundingBox())!;
	const heightBox = (await height.boundingBox())!;
	expect(heightBox.x).toBeGreaterThanOrEqual(wordsBox.x + wordsBox.width);
	expect(heightBox.y).toBeLessThan(wordsBox.y + wordsBox.height);
	expect(heightBox.y + heightBox.height).toBeGreaterThan(wordsBox.y);
	const chipsBox = (await card(page).getByRole('group', { name: 'Layer' }).boundingBox())!;
	expect(heightBox.y).toBeGreaterThan(chipsBox.y + chipsBox.height);
	await mapDrawn();
	await keep('121-map-forecast-wind');

	// The wind at 2000 m.
	await height.selectOption({ label: '2000 m' });
	await mapDrawn();
	await keep('122-map-forecast-wind-2000');

	for (const [layer, name] of [
		['Gusts', '123-map-forecast-gusts'],
		['Cloud', '124-map-forecast-cloud'],
		['Rain', '125-map-forecast-rain']
	] as const) {
		await layerChip(page, layer).click();
		await expect(layerChip(page, layer)).toHaveAttribute('aria-pressed', 'true');
		await expect(hours).toBeVisible();
		await expect(height).toBeHidden();
		await toHour(page, '13:00');
		await mapDrawn();
		await keep(name);
	}

	// The key of each layer. A wide screen has it beside the map, in the
	// screenshots above; on a phone it is unfolded once and stays unfolded
	// from layer to layer, and when the layer is switched off and on.
	if (!wide) {
		await unfoldTheKey(page, wide);
		for (const [layer, name] of [
			['Thermals', '131-map-forecast-key-thermals'],
			['Wind', '132-map-forecast-key-wind'],
			['Gusts', '133-map-forecast-key-gusts'],
			['Cloud', '134-map-forecast-key-cloud'],
			['Rain', '135-map-forecast-key-rain']
		] as const) {
			await layerChip(page, layer).click();
			await expect(layerChip(page, layer)).toHaveAttribute('aria-pressed', 'true');
			await toHour(page, '13:00');
			await expect(keyAndNotes(page)).toHaveAttribute('aria-expanded', 'true');
			const key = card(page).getByRole('list', { name: 'Key' });
			await expect(key.getByRole('listitem').first()).toBeVisible();
			// Folded and unfolded again, for the picture: the card as a tap on
			// "Key and notes" leaves it, with the key at its head.
			await foldTheKey(page, wide);
			await unfoldTheKey(page, wide);
			await mapDrawn();
			await keep(name);
		}
		await forecastChip(page).click();
		await expect(card(page)).toBeHidden();
		await forecastChip(page).click();
		await expect(keyAndNotes(page)).toHaveAttribute('aria-expanded', 'true');
		await foldTheKey(page, wide);
	}

	// A square in an hour: its days side by side, the hour and the layer in
	// the core's words.
	await layerChip(page, 'Wind').click();
	await height.selectOption({ label: 'Ground' });
	if (!wide) await pan(page, mapDrawn, 'ArrowUp', 2);
	await zoom(page, mapDrawn, -out);
	await clickTheMiddle(page, wide);
	const square = wide ? card(page) : page.getByRole('dialog');
	const days = square.getByRole('group', { name: 'This square, day by day' }).getByRole('button');
	await expect(days).toHaveCount(4);
	await expect(days.first()).toHaveAttribute('aria-pressed', 'true');
	await expect(square).toContainText(/km\/h/);
	await expect(square.getByRole('button', { name: 'Forecast for this place' })).toBeVisible();
	await mapDrawn();
	await keep('126-map-forecast-square-hour');

	// Another day from the square's strip: the map shows it.
	await days.nth(1).click();
	await expect(days.nth(1)).toHaveAttribute('aria-pressed', 'true');
	await expect(
		card(page).getByRole('radiogroup', { name: 'Day' }).getByRole('radio').nth(1)
	).toBeChecked();

	// The layer and the height are kept as the pilot left them; the hour is not.
	await page.reload();
	await mapTabDrawn(page, mapDrawn);
	await expect(layerChip(page, 'Wind')).toHaveAttribute('aria-pressed', 'true');
});

test('the wind under the model’s ground is none, and says so', async ({
	context,
	page,
	mapDrawn,
	keep,
	wide
}) => {
	// MADE UP: a square whose made-up ground lies above 1000 m.
	await madeUpMap(page);
	await forecastAt(context, page, mapDrawn, await squareWhereGround(true, 1000));
	await layerChip(page, 'Wind').click();
	await card(page).getByRole('combobox', { name: 'Height' }).selectOption({ label: '1000 m' });
	await expect(card(page).getByRole('slider', { name: 'Hour' })).toBeVisible();
	await mapDrawn();
	await clickTheMiddle(page, wide);
	const square = wide ? card(page) : page.getByRole('dialog');
	await expect(square).toContainText('No wind at 1000 m here');
	await expect(square).toContainText("The model's ground lies higher");
	await mapDrawn();
	await keep('127-map-forecast-wind-under-ground');
});

test('while the hours are asked for the card says so, and when they cannot be had the day stands', async ({
	context,
	page,
	mapDrawn,
	keep,
	wide
}) => {
	await madeUpMap(page);
	// The hours of the wind are slow at first, then refused.
	let answer: 'slow' | 'refused' = 'slow';
	let release = () => {};
	await page.route('**/v1/thermals/hours?*layer=wind*', async (route) => {
		if (answer === 'slow') {
			await new Promise<void>((done) => (release = done));
			return route.fulfill({
				status: 503,
				json: { error: { code: 'unavailable', message: 'for developers' } }
			});
		}
		return route.fulfill({
			status: 503,
			json: { error: { code: 'unavailable', message: 'for developers' } }
		});
	});
	await forecastAt(context, page, mapDrawn, await squareWhereGround(false, 1000));
	await layerChip(page, 'Wind').click();
	await expect(card(page).getByRole('status')).toHaveText(
		'Asking Venti for the hours of this layer.'
	);
	await keep('128-map-forecast-hours-waiting');
	answer = 'refused';
	release();
	await expect(card(page).getByRole('button', { name: 'Try again' })).toBeVisible();
	await mapDrawn();
	await keep('129-map-forecast-hours-refused');
	// The day's squares stand under the day's key.
	await unfoldTheKey(page, wide);
	await expect(card(page).getByRole('list', { name: 'Key' })).toContainText('Big day');
});
