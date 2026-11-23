/**
 * The forecast on the map: the kind of day, square by square, from the
 * server's thermal map, and the wind, the cloud and the rain hour by hour,
 * each along the map's one line of time.
 *
 * The development server's map is a recorded November day of four squares
 * near the Tegernsee, nothing to climb in. Every kind of day is seen with
 * the made-up map of `tools/made-up-day/serve.py`, which
 * `playwright.config.ts` starts in front of the development server: the
 * first test asks it for the map in place of the development server. Its
 * map is made up, and so is everything its screenshots show.
 */

import type { Page } from '@playwright/test';
import {
	BETWEEN_THE_MOUNTAINS,
	expect,
	mapTabDrawn,
	showTheValley,
	test,
	openCredits,
	switchLayers
} from './helpers';

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

/** Goes to a place on the made-up map, with the thermals shown and nothing else of Venti's. */
async function forecastAt(
	context: import('@playwright/test').BrowserContext,
	page: Page,
	mapDrawn: () => Promise<void>,
	place: { latitude: number; longitude: number }
) {
	await context.grantPermissions(['geolocation']);
	await context.setGeolocation(place);
	await page.goto('/map/');
	await mapTabDrawn(page, mapDrawn);
	await switchLayers(page, ['Sites', 'Pilots'], false);
	await what(page, 'Thermals').click();
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

/** The line of time, and the stop it stands on as a screen reader hears it: "Sat, 13:00", "Sat". */
const line = (page: Page) => card(page).getByRole('slider', { name: 'Time' });

/**
 * Steps the line to the first stop whose words end in `ending` ("13:00"),
 * or are a day's name alone (`'day'`), after the one it stands on, with
 * the keyboard.
 */
async function along(page: Page, ending: string | 'day', fromTheStart = true) {
	const stops = line(page);
	const fits = (said: string | null) =>
		ending === 'day' ? Boolean(said) && !said!.includes(',') : Boolean(said?.endsWith(ending));
	// Again while the line is still being read: its days come one by one.
	await expect(async () => {
		await stops.focus();
		if (fromTheStart) await page.keyboard.press('Home');
		for (let step = 0; step < 80; step += 1) {
			await page.keyboard.press('ArrowRight');
			if (fits(await stops.getAttribute('aria-valuetext'))) break;
		}
		expect(fits(await stops.getAttribute('aria-valuetext'))).toBe(true);
	}).toPass();
	await stops.blur();
}

/** A chip of what the map shows. */
const what = (page: Page, name: string) =>
	page
		.getByRole('group', { name: 'What the map shows' })
		.getByRole('button', { name, exact: true });

/** The one panel of what the map shows, at the map's foot. */
const card = (page: Page) => page.getByRole('region', { name: /^(Wind|Thermals|Rain|Cloud)$/ });

/** The panel's "i": the whole key, the model's run and the notes. */
const keyAndNotes = (page: Page) => card(page).getByRole('button', { name: 'Key and notes' });

/** A square chosen on the map: a card at the map's side on a wide screen, a sheet on a phone. */
const squareOf = (page: Page, wide: boolean) =>
	wide ? page.locator('.over aside.card') : page.getByRole('dialog');

/** Unfolds what stands behind the "i", which opens folded. */
async function unfoldTheKey(page: Page) {
	await expect(keyAndNotes(page)).toHaveAttribute('aria-expanded', 'false');
	await keyAndNotes(page).click();
	await expect(keyAndNotes(page)).toHaveAttribute('aria-expanded', 'true');
	await expect(card(page).getByRole('list', { name: 'Key' })).toBeInViewport();
}

/** Folds it away again. */
async function foldTheKey(page: Page) {
	await keyAndNotes(page).click();
	await expect(keyAndNotes(page)).toHaveAttribute('aria-expanded', 'false');
}

test('the thermals show every kind of day, day by day, and what a square says', async ({
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
	await page.goto('/map/');
	await mapTabDrawn(page, mapDrawn);

	// The wind until the pilot chooses; one "what" at a time.
	await expect(what(page, 'Wind')).toHaveAttribute('aria-pressed', 'true');
	await expect(what(page, 'Thermals')).toHaveAttribute('aria-pressed', 'false');
	await what(page, 'Thermals').click();
	await expect(what(page, 'Thermals')).toHaveAttribute('aria-pressed', 'true');
	await expect(what(page, 'Wind')).toHaveAttribute('aria-pressed', 'false');
	// The line opens on the first day as a whole, the kind of day.
	await expect(line(page)).toHaveAttribute('aria-valuenow', '0');
	await expect(line(page)).toHaveAttribute('aria-valuetext', /^[^,]+$/);
	await expect(card(page)).toContainText(/Thermals, [^,]+, forecast, ICON-D2/);
	// The key is a short strip, its ends named, and the day not flyable beside it.
	await expect(card(page)).toContainText('Weak thermals');
	await expect(card(page)).toContainText('Big day');
	await expect(card(page)).toContainText('Not flyable');
	await mapDrawn();
	await keep('112-map-forecast');

	// The whole key, the model and its run behind the "i".
	await unfoldTheKey(page);
	const key = card(page).getByRole('list', { name: 'Key' });
	await expect(key.getByRole('listitem')).toHaveText([
		'No thermals',
		'Weak thermals',
		'Local thermals',
		'Cross-country day',
		'Big day',
		'Not flyable'
	]);
	await expect(card(page)).toContainText(/ICON-D2, run of \w{3}, \d+ \w{3} at \d\d:\d\d/);
	await openCredits(page);
	await expect(page.getByText(/^Forecast: Open-Meteo/)).toBeVisible();
	await keep('130-map-forecast-key-and-notes');
	await foldTheKey(page);

	// The next day, a step weaker everywhere: the next day's cell on the line.
	// Days are chosen by their place: which one is today hangs on the
	// browser's time zone too.
	await along(page, 'day');
	const second = await line(page).getAttribute('aria-valuetext');
	await mapDrawn();
	await keep('113-map-forecast-tomorrow');

	// A square, chosen where nothing else is drawn: the other layers off.
	await switchLayers(page, ['Sites', 'Pilots'], false);
	await showTheValley(page, mapDrawn, 1);
	const canvas = page.locator('canvas.maplibregl-canvas');
	const box = (await canvas.boundingBox())!;
	// Over the valley, above the panel at the map's foot.
	await canvas.click({ position: { x: box.width / 2, y: box.height * 0.3 } });
	const square = squareOf(page, wide);
	await expect(square).toContainText(/Judged at the model’s ground, \d+ m/);
	await expect(square).toContainText(/Flyable|Marginal|Not flyable/);
	await expect(square).toContainText(/m\/s|None to climb in|Not flyable/);
	// The square's days side by side at its head: the day the map shows is marked.
	const strip = square.getByRole('group', { name: 'This square, day by day' }).getByRole('button');
	await expect(strip).toHaveCount(4);
	await expect(strip.nth(1)).toHaveAttribute('aria-pressed', 'true');
	await mapDrawn();
	await keep('114-map-forecast-square');

	// Another day from the strip: the map and the line show it, and the
	// square stays open with that day's reading. Then back to the second day.
	await strip.nth(2).click();
	await expect(strip.nth(2)).toHaveAttribute('aria-pressed', 'true');
	await expect(line(page)).not.toHaveAttribute('aria-valuetext', second!);
	await expect(square).toContainText(/Judged at the model’s ground, \d+ m/);
	await expect(square).toContainText('ICON-EU');
	await strip.nth(1).click();
	await expect(strip.nth(1)).toHaveAttribute('aria-pressed', 'true');
	await expect(line(page)).toHaveAttribute('aria-valuetext', second!);
	await expect(square).toContainText('ICON-D2');

	// The hours behind the day's verdict: the forecast for the square's middle,
	// open on the day the map shows, the second, of the model the map read it from.
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

	// Pressed again, the map shows nothing over the ground, and the choice is kept.
	if (!wide) await page.getByRole('dialog').getByRole('button', { name: 'Done' }).click();
	await what(page, 'Thermals').click();
	await expect(card(page)).toBeHidden();
	await page.reload();
	await mapTabDrawn(page, mapDrawn);
	for (const name of ['Wind', 'Thermals', 'Rain', 'Cloud']) {
		await expect(what(page, name)).toHaveAttribute('aria-pressed', 'false');
	}
});

test('the thermals read the development server’s November map, and say when there is none', async ({
	context,
	page,
	mapDrawn,
	wide
}) => {
	await context.grantPermissions(['geolocation']);
	await context.setGeolocation(BETWEEN_THE_MOUNTAINS);
	// First the server has no map: the panel says so in the core's words.
	let refusing = true;
	await page.route('**/v1/thermals', async (route) => {
		if (refusing) await route.fulfill({ status: 503, body: '' });
		else await route.fallback();
	});
	await page.goto('/map/');
	await mapTabDrawn(page, mapDrawn);
	await what(page, 'Thermals').click();
	await expect(card(page).getByRole('button', { name: 'Try again' })).toBeVisible();
	await expect(line(page)).toBeHidden();

	// Asked again, it has the recorded map: a day not flyable, then weak thermals.
	refusing = false;
	await card(page).getByRole('button', { name: 'Try again' }).click();
	await expect(line(page)).toHaveAttribute('aria-valuetext', 'Sat');
	await unfoldTheKey(page);
	await expect(card(page)).toContainText('ICON-D2, run of Sat, 21 Nov at 07:00');
	await foldTheKey(page);
	// Its four days along the line, the last ICON-EU's.
	await line(page).focus();
	await page.keyboard.press('End');
	await expect(line(page)).toHaveAttribute('aria-valuetext', /^Tue/);
	await expect(card(page)).toContainText('ICON-EU');
	await page.keyboard.press('Home');
	await line(page).blur();

	// A square of the first day, around the pilot: not flyable, for rain.
	await switchLayers(page, ['Sites', 'Pilots'], false);
	await showTheValley(page, mapDrawn, 1);
	await clickTheMiddle(page, wide);
	const square = squareOf(page, wide);
	await expect(square).toContainText('Not flyable');
	await expect(square).toContainText('Rain');
	await expect(square).toContainText(/Judged at the model’s ground, \d+ m/);
	if (!wide) await page.getByRole('dialog').getByRole('button', { name: 'Done' }).click();
	await switchLayers(page, ['Sites', 'Pilots'], true);

	// Kept as the pilot left it.
	await page.reload();
	await mapTabDrawn(page, mapDrawn);
	await expect(what(page, 'Thermals')).toHaveAttribute('aria-pressed', 'true');
	await expect(line(page)).toBeVisible();
});

test('the wind, the cloud and the rain hour by hour, and what a square says in an hour', async ({
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
	// On a phone the panel lies over the map's foot: the view goes south,
	// so that the field stands in what the panel leaves free.
	if (!wide) await pan(page, mapDrawn, 'ArrowDown', 2);

	// One hour on from the day as a whole: that hour's thermals, under the
	// hour's key. A step of an hour is measured from the key to the map drawn again.
	await expect(line(page)).toHaveAttribute('aria-valuetext', /^[^,]+$/);
	await line(page).focus();
	const started = Date.now();
	await page.keyboard.press('ArrowRight');
	await expect(line(page)).toHaveAttribute('aria-valuetext', /, \d\d:00$/);
	await mapDrawn();
	console.log(`A step of an hour took ${Date.now() - started} ms to draw.`);
	await line(page).blur();
	// Midday, where the made-up day has the most to see.
	await along(page, '13:00');
	await mapDrawn();
	await keep('120-map-forecast-thermals-hour');

	// The wind: it opens on what the stations measure now, and runs on into
	// the forecast, the wind at the ground, with arrows.
	await what(page, 'Wind').click();
	await expect(card(page)).toContainText('Wind, now, measured');
	await expect(line(page)).toHaveAttribute('aria-valuetext', 'Now');
	await along(page, '13:00');
	const height = card(page).getByRole('combobox', { name: 'Height' });
	await expect(height).toHaveValue('');
	await expect(card(page)).toContainText(/Wind at the ground, (\w+ )?13:00, forecast, ICON-D2/);
	await mapDrawn();
	await keep('121-map-forecast-wind');

	// The wind at 2000 m.
	await height.selectOption({ label: '2000 m' });
	await expect(card(page)).toContainText(/Wind at 2000 m, (\w+ )?13:00, forecast/);
	await mapDrawn();
	await keep('122-map-forecast-wind-2000');

	// The gusts, a switch of the wind's forecast, at the ground alone.
	const gusts = card(page).getByRole('button', { name: 'Gusts', exact: true });
	await gusts.click();
	await expect(gusts).toHaveAttribute('aria-pressed', 'true');
	await expect(height).toBeHidden();
	await along(page, '13:00');
	await expect(card(page)).toContainText(/Gusts at the ground, (\w+ )?13:00, forecast/);
	await mapDrawn();
	await keep('123-map-forecast-gusts');
	await gusts.click();
	await expect(gusts).toHaveAttribute('aria-pressed', 'false');

	for (const [name, picture] of [
		['Cloud', '124-map-forecast-cloud'],
		['Rain', '125-map-forecast-rain']
	] as const) {
		await what(page, name).click();
		await expect(what(page, name)).toHaveAttribute('aria-pressed', 'true');
		await along(page, '13:00');
		await expect(card(page)).toContainText(new RegExp(`${name}, (\\w+ )?13:00, forecast`));
		await mapDrawn();
		await keep(picture);
	}
	// The hour's whole key and the layer's notes behind the "i".
	await unfoldTheKey(page);
	await expect(card(page).getByRole('list', { name: 'Key' }).getByRole('listitem')).toHaveText([
		'Light rain',
		'Rain',
		'Heavy rain'
	]);
	await mapDrawn();
	await keep('131-map-forecast-key-and-notes-hour');
	await foldTheKey(page);

	// A square in an hour: its days side by side, the hour and the layer in
	// the core's words.
	await what(page, 'Wind').click();
	await along(page, '13:00');
	await height.selectOption({ label: 'Ground' });
	if (!wide) await pan(page, mapDrawn, 'ArrowUp', 2);
	await zoom(page, mapDrawn, -out);
	await clickTheMiddle(page, wide);
	const square = squareOf(page, wide);
	const days = square.getByRole('group', { name: 'This square, day by day' }).getByRole('button');
	await expect(days).toHaveCount(4);
	// The day the line stands on is marked: today's 13:00 is past once the
	// development server's clock has passed it, and then it is tomorrow's.
	await expect(
		square.getByRole('group', { name: 'This square, day by day' }).getByRole('button', {
			pressed: true
		})
	).toHaveCount(1);
	await expect(square).toContainText(/km\/h/);
	await expect(square.getByRole('button', { name: 'Forecast for this place' })).toBeVisible();
	await mapDrawn();
	await keep('126-map-forecast-square-hour');

	// Another day from the square's strip: the line goes to it.
	const before = await line(page).getAttribute('aria-valuetext');
	await days.nth(3).click();
	await expect(days.nth(3)).toHaveAttribute('aria-pressed', 'true');
	await expect(line(page)).not.toHaveAttribute('aria-valuetext', before!);

	// What the map shows and the height are kept as the pilot left them; the hour is not.
	await page.reload();
	await mapTabDrawn(page, mapDrawn);
	await expect(what(page, 'Wind')).toHaveAttribute('aria-pressed', 'true');
	await expect(line(page)).toHaveAttribute('aria-valuetext', 'Now');
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
	await what(page, 'Wind').click();
	await along(page, '13:00');
	await card(page).getByRole('combobox', { name: 'Height' }).selectOption({ label: '1000 m' });
	await mapDrawn();
	await clickTheMiddle(page, wide);
	const square = squareOf(page, wide);
	await expect(square).toContainText('No wind at 1000 m here');
	await expect(square).toContainText("The model's ground lies higher");
	await mapDrawn();
	await keep('127-map-forecast-wind-under-ground');
});

test('while the hours are asked for the panel says so, and when they cannot be had it says that', async ({
	context,
	page,
	mapDrawn,
	keep
}) => {
	await madeUpMap(page);
	// The hours of the cloud are slow at first, then refused: every day's,
	// which the line asks for at once.
	const waiting: (() => void)[] = [];
	let slow = true;
	await page.route('**/v1/thermals/hours?*layer=cloud*', async (route) => {
		if (slow) await new Promise<void>((done) => waiting.push(done));
		return route.fulfill({
			status: 503,
			json: { error: { code: 'unavailable', message: 'for developers' } }
		});
	});
	await forecastAt(context, page, mapDrawn, await squareWhereGround(false, 1000));
	await what(page, 'Cloud').click();
	await expect(card(page)).toContainText('Asking Venti for the hours of this layer.');
	await keep('128-map-forecast-hours-waiting');
	slow = false;
	for (const release of waiting) release();
	await expect(card(page).getByRole('button', { name: 'Try again' })).toBeVisible();
	await mapDrawn();
	await keep('129-map-forecast-hours-refused');
});

/** The fans the map tab last drew, with the name of each one's picture: what it says and its spans. */
const fans = (page: Page) =>
	page.evaluate(
		() =>
			(globalThis as unknown as { ventiFans?: { fans: { id: string; picture: string }[] } })
				.ventiFans?.fans ?? []
	);

test('the take-offs are fans over their wind directions, coloured by the forecast’s wind at the ground', async ({
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
	await switchLayers(page, ['Sites'], true);
	// A phone shows less of the valley: a step out brings take-offs that name their wind.
	if (!wide) await zoom(page, mapDrawn, -1);
	await mapDrawn();

	// Without the wind's forecast every fan is in one neutral ink.
	await expect.poll(async () => (await fans(page)).length).toBeGreaterThan(0);
	expect((await fans(page)).every((fan) => fan.picture.startsWith('site-fan:neutral:'))).toBe(true);
	// Landings have their own sign and no fan.
	expect((await fans(page)).map((fan) => fan.picture)).not.toContain(undefined);

	// The wind at the ground at 13:00: a fan the wind fits, and one it does not, in their colours.
	await what(page, 'Wind').click();
	await along(page, '13:00');
	await expect(card(page)).toContainText(/Wind at the ground, (\w+ )?13:00, forecast/);
	await expect
		.poll(
			async () =>
				(await fans(page)).filter((fan) => !fan.picture.startsWith('site-fan:neutral:')).length
		)
		.toBeGreaterThan(0);
	// The key says what the colours mean.
	await expect(card(page).locator('.fans')).toContainText('Take-off fits');
	await expect(card(page).locator('.fans')).toContainText('Does not fit');
	await mapDrawn();
	await keep('300-map-take-off-fans');
	await page.emulateMedia({ colorScheme: 'dark' });
	await mapDrawn();
	await keep('301-map-take-off-fans-dark');
	await page.emulateMedia({ colorScheme: test.info().project.use.colorScheme ?? 'light' });

	// At a height the wind is not the ground's: the fans are neutral again.
	await card(page).getByRole('combobox', { name: 'Height' }).selectOption({ label: '2000 m' });
	await expect(card(page)).toContainText(/Wind at 2000 m/);
	await expect
		.poll(async () =>
			(await fans(page)).every((fan) => fan.picture.startsWith('site-fan:neutral:'))
		)
		.toBe(true);
	await expect(card(page).locator('.fans')).toHaveCount(0);
});
