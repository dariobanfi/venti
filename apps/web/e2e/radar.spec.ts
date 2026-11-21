/**
 * The map's "Radar" layer: the last frames of rain a weather service's
 * radars saw, as pictures over the map.
 *
 * The development server has two frames, recorded from DWD on 21 November
 * 2026 at 11:35 and 11:40 UTC, a morning with little rain: the pictures
 * are mostly "no rain" and the veil of what the radars did not see. Its
 * radar's clock stands at 11:44, so the newest frame is four minutes old.
 * The browser's clock is Berlin's: 12:35 and 12:40.
 *
 * Outside the Tegernsee valley the test's map is blank, so the picture
 * lies over an empty ground here. That is the test's map, not the layer.
 */

import type { Page } from '@playwright/test';
import { expect, mapTabDrawn, test } from './helpers';

const radarChip = (page: Page) =>
	page.getByRole('group', { name: 'Layers on the map' }).getByRole('button', { name: 'Radar' });

/** The card of the layer: a pane beside the map on a wide screen, a card at its foot on a phone. */
const card = (page: Page) => page.getByRole('region', { name: 'Rain radar' });

const strip = (page: Page) => card(page).getByRole('slider', { name: 'Radar picture' });

const keyAndNotes = (page: Page) => card(page).getByRole('button', { name: 'Key and notes' });

/** The list of frames as the development server answers it. */
async function recordedList(page: Page) {
	const response = await page.request.get('/v1/radar');
	return (await response.json()) as { time: string; layers: { frames: { path: string }[] }[] };
}

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

test('the radar layer shows the newest picture with its time, the frames, the key and whose it is', async ({
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

	// It opens on the newest frame: the strip marks it, and the card says
	// its time and its age in the core's words.
	await expect(card(page)).toContainText('Rain radar, 12:40, 4 minutes ago');
	await expect(strip(page)).toHaveAttribute('aria-valuemax', '1');
	await expect(strip(page)).toHaveAttribute('aria-valuenow', '1');
	await expect(strip(page)).toHaveAttribute('aria-valuetext', '12:40, 4 minutes ago');
	// The newest frame's picture alone was fetched.
	expect(pictures).toEqual(['/v1/radar/dwd-radar-ry/20261121T1140Z.png']);
	// DWD asks for its credit at the data: on the map while the layer shows.
	await expect(
		page
			.locator('.credits')
			.getByText('Datenbasis: Deutscher Wetterdienst, Rasterdaten bildlich wiedergegeben')
	).toBeVisible();

	// Far enough out to see where the picture ends and what the radars do not see.
	await zoomOut(page, mapDrawn, 2);
	await keep('136-map-radar');

	// The key is the core's: the steps from the most rain down, the veil
	// last, then what a radar does not see and whose pictures they are. On
	// a phone it is one tap away.
	const key = card(page).getByRole('list', { name: 'Key' });
	if (wide) {
		await expect(keyAndNotes(page)).toBeHidden();
	} else {
		await expect(key).toBeHidden();
		await keyAndNotes(page).click();
		await expect(keyAndNotes(page)).toHaveAttribute('aria-expanded', 'true');
	}
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
	await expect(
		card(page).getByRole('link', {
			name: 'Datenbasis: Deutscher Wetterdienst, Rasterdaten bildlich wiedergegeben'
		})
	).toBeVisible();
	await expect(card(page)).toContainText('CC BY 4.0');
	await keep('137-map-radar-key-and-notes');
	if (!wide) {
		await keyAndNotes(page).click();
		await expect(key).toBeHidden();
	}

	// A step to the frame before, with the keyboard: its picture is fetched
	// and the card says its time.
	await strip(page).focus();
	await page.keyboard.press('ArrowLeft');
	await expect(strip(page)).toHaveAttribute('aria-valuetext', '12:35, 9 minutes ago');
	await expect(card(page)).toContainText('Rain radar, 12:35, 9 minutes ago');
	expect(pictures).toEqual([
		'/v1/radar/dwd-radar-ry/20261121T1140Z.png',
		'/v1/radar/dwd-radar-ry/20261121T1135Z.png'
	]);
	await strip(page).blur();
	await mapDrawn();
	await keep('138-map-radar-frame-before');

	// Played, the frames run once and stop at the newest. Neither picture
	// is asked for again: they are kept for the visit.
	await card(page).getByRole('button', { name: 'Play' }).click();
	await expect(strip(page)).toHaveAttribute('aria-valuenow', '1');
	await expect(card(page)).toContainText('Rain radar, 12:40, 4 minutes ago');
	await expect(card(page).getByRole('button', { name: 'Play' })).toBeVisible();
	expect(pictures).toHaveLength(2);

	// The layer is kept as the pilot left it.
	await page.reload();
	await mapTabDrawn(page, mapDrawn);
	await expect(radarChip(page)).toHaveAttribute('aria-pressed', 'true');
	await expect(card(page)).toContainText('Rain radar, 12:40, 4 minutes ago');
	await radarChip(page).click();
	await expect(card(page)).toBeHidden();
	await expect(
		page.locator('.credits').getByText('Datenbasis: Deutscher Wetterdienst')
	).toBeHidden();

	expect(problems).toEqual([]);
});

test('an old picture is not shown as the rain of now, and a picture that cannot be had leaves the one before', async ({
	page,
	mapDrawn,
	keep
}) => {
	// MADE UP: the recorded list, answered as if half an hour had gone by
	// since its newest frame; and the server has lost the frame before.
	const list = await recordedList(page);
	await page.route('**/v1/radar', (route) =>
		route.fulfill({ json: { ...list, time: '2026-11-21T12:10:00Z' } })
	);
	await page.route('**/v1/radar/dwd-radar-ry/20261121T1135Z.png', (route) =>
		route.fulfill({
			status: 404,
			json: { error: { code: 'not_found', message: 'no such frame' } }
		})
	);
	await radarOn(page, mapDrawn);

	await expect(card(page)).toContainText(
		'The newest radar picture is 30 minutes old. It is not the rain of now.'
	);
	// The picture keeps its own time under the warning.
	await expect(card(page)).toContainText('Rain radar, 12:40, 30 minutes ago');
	await zoomOut(page, mapDrawn, 1);
	await keep('139-map-radar-old');

	// The frame before cannot be had: the card says so, and the picture on
	// the map is still the one of 12:40, named as that.
	await strip(page).focus();
	await page.keyboard.press('ArrowLeft');
	await expect(strip(page)).toHaveAttribute('aria-valuetext', '12:35, 35 minutes ago');
	await expect(card(page)).toContainText('The radar picture could not be read.');
	await expect(card(page)).toContainText('Rain radar, 12:40, 30 minutes ago');
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
	await expect(card(page)).toContainText('Rain radar, 12:40, 4 minutes ago');
});
