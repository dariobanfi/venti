/**
 * The map's "Radar" layer: the rain of the last hours as pictures over the
 * map, a layer for each source along one line of time.
 *
 * The development server has a recording of both sources of the first
 * region, made within minutes of each other on 21 November 2026: two
 * frames of DWD's radars, five minutes apart, which see Bavaria and the
 * north of Tirol, and two of GeoSphere Austria's analysis, a quarter of an
 * hour apart and older, which sees the whole region. Its radar's clock
 * stands four minutes after the radars' newer frame. Which frames those
 * are is read from the server's list here, so that a new recording needs
 * no new figures; the browser's clock is Berlin's.
 *
 * Outside the Tegernsee valley the test's map is blank, so the pictures
 * lie over an empty ground here. That is the test's map, not the layer.
 */

import type { Page } from '@playwright/test';
import { expect, mapTabDrawn, test } from './helpers';

const radarChip = (page: Page) =>
	page.getByRole('group', { name: 'Layers on the map' }).getByRole('button', { name: 'Radar' });

/** The card of the layer: a pane beside the map on a wide screen, a card at its foot on a phone. */
const card = (page: Page) => page.getByRole('region', { name: 'Rain radar' });

const strip = (page: Page) => card(page).getByRole('slider', { name: 'Radar picture' });

const keyAndNotes = (page: Page) => card(page).getByRole('button', { name: 'Key and notes' });

interface Recorded {
	time: string;
	layers: { source: string; frames: { time: string; path: string }[] }[];
}

/** The list of frames as the development server answers it. */
async function recordedList(page: Page) {
	const response = await page.request.get('/v1/radar');
	return (await response.json()) as Recorded;
}

const RADARS = 'dwd-radar-ry';
const ANALYSIS = 'geosphere-inca-rr';
const RADARS_CREDIT = 'Datenbasis: Deutscher Wetterdienst, Rasterdaten bildlich wiedergegeben';
const ANALYSIS_CREDIT = 'Data source: GeoSphere Austria - https://data.hub.geosphere.at';

/** A source's frames in the list, the oldest first. */
const framesOf = (list: Recorded, source: string) =>
	list.layers.find((layer) => layer.source === source)!.frames;

/** A moment on the browser's clock, Berlin's: "14:15". */
const clock = (time: string) =>
	new Date(time).toLocaleTimeString('en-GB', {
		timeZone: 'Europe/Berlin',
		hour: '2-digit',
		minute: '2-digit'
	});

/** The analysis' frame that lies under a moment: its newest that is not of a later one. */
const analysisUnder = (list: Recorded, time: string) =>
	framesOf(list, ANALYSIS).findLast((frame) => Date.parse(frame.time) <= Date.parse(time))!;

/**
 * The core's line for a moment of the radars' frame of `time`, whatever
 * its age: "Rain radar, 14:15, 4 minutes ago. Austria's analysis is of
 * 14:00."
 */
const lineOf = (list: Recorded, time: string, age = String.raw`\d+ minutes ago`) =>
	new RegExp(
		`Rain radar, ${clock(time)}, ${age}\\. Austria's analysis is of ${clock(analysisUnder(list, time).time)}\\.`
	);

/** The radar's layers the map draws, the lowest first, as the map tab leaves them for a test. */
const drawnOnMap = (page: Page) =>
	page.evaluate(() => (globalThis as { ventiRadar?: string[] }).ventiRadar);

/** Opens the map tab with nothing of Venti's on it but the radar, and waits for its picture. */
async function radarOn(page: Page, mapDrawn: () => Promise<void>) {
	await page.goto('/');
	await mapTabDrawn(page, mapDrawn);
	for (const name of ['Wind', 'Sites', 'Pilots']) {
		await page
			.getByRole('group', { name: 'Layers on the map' })
			.getByRole('button', { name })
			.click();
	}
	await radarChip(page).click();
}

/** Zooms the map out by steps, with the keyboard, and waits for it to be drawn. */
async function zoomOut(page: Page, mapDrawn: () => Promise<void>, steps: number) {
	const canvas = page.locator('canvas.maplibregl-canvas');
	await canvas.focus();
	for (let step = 0; step < steps; step += 1) {
		await page.keyboard.press('Minus');
		await mapDrawn();
	}
	await canvas.blur();
}

test('the radar layer shows both layers at the newest moment with its time, the moments, the key and whose they are', async ({
	page,
	mapDrawn,
	keep,
	wide,
	problems
}) => {
	const pictures: string[] = [];
	page.on('request', (request) => {
		const { pathname } = new URL(request.url());
		if (pathname.startsWith('/v1/radar/')) pictures.push(pathname);
	});
	const list = await recordedList(page);
	const stops = framesOf(list, RADARS);
	const newest = stops.at(-1)!;
	const before = stops.at(-2)!;
	await page.goto('/');
	await mapTabDrawn(page, mapDrawn);

	// Off until the pilot switches it on, and nothing is asked before.
	await expect(radarChip(page)).toHaveAttribute('aria-pressed', 'false');
	await expect(card(page)).toBeHidden();
	expect(pictures).toEqual([]);
	for (const name of ['Wind', 'Sites', 'Pilots']) {
		await page
			.getByRole('group', { name: 'Layers on the map' })
			.getByRole('button', { name })
			.click();
	}
	await radarChip(page).click();
	await expect(radarChip(page)).toHaveAttribute('aria-pressed', 'true');

	// It opens on the newest moment, a stop for each of the radars' frames:
	// the strip marks it, and the card says its time, its age and what lies
	// under it, in the core's words.
	await expect(card(page)).toContainText(lineOf(list, newest.time));
	await expect(strip(page)).toHaveAttribute('aria-valuemax', String(stops.length - 1));
	await expect(strip(page)).toHaveAttribute('aria-valuenow', String(stops.length - 1));
	await expect(strip(page)).toHaveAttribute('aria-valuetext', lineOf(list, newest.time));
	// The pictures of that moment alone were fetched: the radars', and the
	// analysis' that lies under it.
	expect(pictures.toSorted()).toEqual(
		[analysisUnder(list, newest.time).path, newest.path].toSorted()
	);
	// Each is a layer of its own on the map, the analysis under the radars.
	await expect.poll(() => drawnOnMap(page)).toEqual(['radar-0', 'radar-1']);
	// Both ask for their credit at the data: on the map while they show.
	await expect(page.locator('.credits').getByText(RADARS_CREDIT)).toBeVisible();
	await expect(page.locator('.credits').getByText(ANALYSIS_CREDIT)).toBeVisible();

	// Far enough out to see the whole region: the radars see its north-west,
	// and the analysis fills what they do not see.
	await zoomOut(page, mapDrawn, wide ? 2 : 3);
	await keep('136-map-radar');

	// What each layer is, a line a layer, then the core's key: the steps
	// from the most rain down, the veil last, then what a radar does not
	// see and whose pictures they are. On a phone it is one tap away.
	const layers = card(page).getByRole('list', { name: 'Layers' });
	const key = card(page).getByRole('list', { name: 'Key' });
	if (wide) {
		await expect(keyAndNotes(page)).toBeHidden();
	} else {
		await expect(key).toBeHidden();
		await expect(layers).toBeHidden();
		await keyAndNotes(page).click();
		await expect(keyAndNotes(page)).toHaveAttribute('aria-expanded', 'true');
	}
	await expect(layers.getByRole('listitem')).toHaveText([
		"Austria's analysis of radar and gauges, every 15 minutes",
		"Germany's radars, every 5 minutes"
	]);
	await expect(key.getByRole('listitem')).toHaveText([
		'Cloudburst, 30 mm an hour and more',
		'Very heavy rain, 10 to 30 mm an hour',
		'Heavy rain, 4 to 10 mm an hour',
		'Rain, 1 to 4 mm an hour',
		'Light rain, 0.2 to 1 mm an hour',
		'A trace, under 0.2 mm an hour',
		'Not seen by the radar'
	]);
	await expect(card(page)).toContainText('It is no forecast');
	await expect(card(page)).toContainText('such places are marked as not seen, which is not dry');
	await expect(card(page)).toContainText("the radar's picture lies on top");
	await expect(card(page).getByRole('link', { name: RADARS_CREDIT })).toBeVisible();
	await expect(card(page).getByRole('link', { name: ANALYSIS_CREDIT })).toBeVisible();
	await expect(card(page)).toContainText('CC BY 4.0');
	await keep('137-map-radar-key-and-notes');
	if (!wide) {
		await keyAndNotes(page).click();
		await expect(key).toBeHidden();
	}

	// A step to the moment before, with the keyboard: the radars' picture of
	// it is fetched and the card says its time. The analysis under it is the
	// one already fetched, and the server is not asked for it again: the
	// worker has it as it was sent, and colours it anew under these radars.
	await strip(page).focus();
	await page.keyboard.press('ArrowLeft');
	await expect(strip(page)).toHaveAttribute('aria-valuetext', lineOf(list, before.time));
	await expect(card(page)).toContainText(lineOf(list, before.time));
	const fetched = [
		...new Set([
			analysisUnder(list, newest.time).path,
			newest.path,
			analysisUnder(list, before.time).path,
			before.path
		])
	];
	expect(pictures.toSorted()).toEqual(fetched.toSorted());
	await expect.poll(() => drawnOnMap(page)).toEqual(['radar-0', 'radar-1']);
	await strip(page).blur();
	await mapDrawn();
	await keep('138-map-radar-frame-before');

	// Played, the moments run once and stop at the newest. No picture is
	// asked for again: they are kept for the visit.
	await card(page).getByRole('button', { name: 'Play' }).click();
	await expect(strip(page)).toHaveAttribute('aria-valuenow', String(stops.length - 1));
	await expect(card(page)).toContainText(lineOf(list, newest.time));
	await expect(card(page).getByRole('button', { name: 'Play' })).toBeVisible();
	expect(pictures).toHaveLength(fetched.length);

	// The layer is kept as the pilot left it.
	await page.reload();
	await mapTabDrawn(page, mapDrawn);
	await expect(radarChip(page)).toHaveAttribute('aria-pressed', 'true');
	await expect(card(page)).toContainText(lineOf(list, newest.time));
	await radarChip(page).click();
	await expect(card(page)).toBeHidden();
	await expect(
		page.locator('.credits').getByText('Datenbasis: Deutscher Wetterdienst')
	).toBeHidden();
	await expect(page.locator('.credits').getByText(ANALYSIS_CREDIT)).toBeHidden();
	await expect.poll(() => drawnOnMap(page)).toEqual([]);

	expect(problems).toEqual([]);
});

test('old rain is not shown as the rain of now, and a picture that cannot be had leaves the moment before', async ({
	page,
	mapDrawn,
	keep,
	wide
}) => {
	// MADE UP: the recorded list, answered as if half an hour had gone by
	// since the radars' newest frame; and the server has lost their frame
	// before it.
	const list = await recordedList(page);
	const stops = framesOf(list, RADARS);
	const newest = stops.at(-1)!;
	const before = stops.at(-2)!;
	const later = new Date(Date.parse(newest.time) + 30 * 60_000).toISOString();
	await page.route('**/v1/radar', (route) => route.fulfill({ json: { ...list, time: later } }));
	await page.route(`**${before.path}`, (route) =>
		route.fulfill({
			status: 404,
			json: { error: { code: 'not_found', message: 'no such frame' } }
		})
	);
	await radarOn(page, mapDrawn);

	// The caution is the topmost layer's, the radars': a quarter of an hour
	// is their patience. The analysis under them has three.
	await expect(card(page)).toContainText(
		'The newest radar picture is 30 minutes old. It is not the rain of now.'
	);
	// The moment keeps its own time under the warning.
	await expect(card(page)).toContainText(lineOf(list, newest.time, '30 minutes ago'));
	await expect.poll(() => drawnOnMap(page)).toEqual(['radar-0', 'radar-1']);
	await zoomOut(page, mapDrawn, wide ? 2 : 3);
	await keep('139-map-radar-old');

	// The radars' frame before cannot be had: the card says so, and what is
	// on the map is still the newest moment whole, named as that.
	await strip(page).focus();
	await page.keyboard.press('ArrowLeft');
	await expect(strip(page)).toHaveAttribute(
		'aria-valuetext',
		lineOf(list, before.time, '35 minutes ago')
	);
	await expect(card(page)).toContainText('The radar picture could not be read.');
	await expect(card(page)).toContainText(lineOf(list, newest.time, '30 minutes ago'));
	await expect(card(page).getByRole('button', { name: 'Try again' })).toBeVisible();
	await strip(page).blur();
	await keep('140-map-radar-picture-missing');
});

test('a server without a radar says so on the card', async ({ page, mapDrawn, keep }) => {
	// MADE UP: the answer of a server none of whose regions names a radar source.
	let asked = 0;
	await page.route('**/v1/radar', (route) => {
		asked += 1;
		return route.fulfill({
			status: 501,
			json: { error: { code: 'no_radar', message: 'this server collects no rain radar' } }
		});
	});
	await radarOn(page, mapDrawn);

	await expect(card(page).getByRole('status')).toContainText('This server has no rain radar.');
	await expect(strip(page)).toBeHidden();
	await expect(page.locator('.credits').getByText('Datenbasis')).toBeHidden();
	await keep('141-map-radar-none');

	// "Try again" asks the server again.
	const before = asked;
	await card(page).getByRole('button', { name: 'Try again' }).click();
	await expect.poll(() => asked).toBeGreaterThan(before);
	await expect(card(page).getByRole('status')).toContainText('This server has no rain radar.');
});

test('a server that cannot be reached says so, and the radar comes when it can', async ({
	page,
	mapDrawn
}) => {
	let reachable = false;
	await page.route('**/v1/radar', (route) => (reachable ? route.continue() : route.abort()));
	await radarOn(page, mapDrawn);

	await expect(card(page).getByRole('status')).toContainText(
		'Venti could not be reached for the rain radar.'
	);
	reachable = true;
	await card(page).getByRole('button', { name: 'Try again' }).click();
	await expect(card(page)).toContainText(/Rain radar, \d\d:\d\d, \d+ minutes ago/);
});
