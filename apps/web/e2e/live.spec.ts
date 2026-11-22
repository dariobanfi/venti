/**
 * Who is in the air, from the Venti server in its development mode: two
 * gliders of the demo pilot over the Tegernsee, one flying and one that has
 * landed, with the server's clock standing at 18 November 2026, 19:05 UTC.
 * `playwright.config.ts` starts the server.
 */

import type { BrowserContext, Locator, Page } from '@playwright/test';
import { expect, mapTabDrawn, metric, showMyPosition, test, mapTool } from './helpers';

/** The tracks of the two gliders, and where each was last heard of. */
const FLYING = { id: '11111111111111111111111111', latitude: 47.6637733, longitude: 11.7431677 };
const LANDED = { id: '22222222222222222222222222', latitude: 47.6921681, longitude: 11.7674868 };

/** Between the two, for the pilot to stand at: both are on a phone's map from there. */
const BETWEEN = { latitude: 47.678, longitude: 11.755 };

/** How many gliders a map says it shows, for those who cannot see it. */
const shown = (page: Page) => page.getByText(/^In the air on this part of the map/);

/** A chosen glider: a sheet on a narrow screen, a card beside the map on a wide one. */
function gliderPanel(page: Page, wide: boolean): Locator {
	return page.getByRole(wide ? 'complementary' : 'dialog', { name: '@demo' });
}

const closeButton = (panel: Locator, wide: boolean) =>
	panel.getByRole('button', { name: wide ? 'Close' : 'Done' });

/**
 * Opens the map where the pilot stands, close enough to tell the gliders
 * apart, and waits for the gliders that are `inView` there.
 */
async function mapAt(
	context: BrowserContext,
	page: Page,
	mapDrawn: () => Promise<void>,
	place: { latitude: number; longitude: number },
	inView = 2
): Promise<void> {
	await context.grantPermissions(['geolocation']);
	await context.setGeolocation(place);
	await page.goto('/map/');
	await mapTabDrawn(page, mapDrawn);
	await showMyPosition(page, mapDrawn);
	await expect(shown(page)).toHaveText(`In the air on this part of the map: ${inView}.`);
	// The map places the wings over a few frames and fades their names in.
	await page.waitForTimeout(1500);
}

/** Clicks the middle of what the navigation leaves of the map, a little off: a finger is not exact. */
async function clickTheMiddle(page: Page): Promise<void> {
	const map = (await page.locator('canvas.maplibregl-canvas').boundingBox())!;
	const covered = await page.locator('.navigation-height').evaluate((bar) => bar.clientHeight);
	await page.mouse.click(map.x + map.width / 2 + 8, map.y + (map.height - covered) / 2 - 8);
}

test('the map draws who is in the air, lists them, and follows the flight of one', async ({
	context,
	page,
	mapDrawn,
	keep,
	wide,
	problems
}) => {
	/** What the server answered to each question about gliders, in order. */
	const answers: { path: string; status: number }[] = [];
	page.on('response', (response) => {
		const address = new URL(response.url());
		if (address.pathname.startsWith('/v1/live')) {
			answers.push({ path: address.pathname + address.search, status: response.status() });
		}
	});

	await mapAt(context, page, mapDrawn, BETWEEN);
	// A wing turned the way it flies, a disc on the ground, and under each
	// the pilot's name and altitude.
	await keep('54-map-live');

	// Everyone on the map as a list: who flies comes first.
	await mapTool(page, 'Pilots in the air');
	const list = page.getByRole('dialog', { name: 'In the air' });
	const rows = list.getByRole('listitem');
	await expect(rows).toHaveCount(2);
	await expect(rows.first()).toContainText('Demo Pilot');
	// The age is in the core's words, counted on by this browser's clock.
	await expect(rows.first()).toContainText(/1596 m · \u22121\.0 m\/s · (Now|\d+ s ago)$/);
	await expect(rows.last()).toContainText(/786 m · Landed (9|10) min ago$/);
	await expect(list).toContainText(
		'A glider nothing was heard of for a while has most likely no network: it is not known to have landed.'
	);
	await keep('55-live-list');

	// Chosen from the list: the glider's page, and its flight on the map.
	await rows.first().getByRole('button').click();
	await expect(list).toBeHidden();
	const glider = gliderPanel(page, wide);
	await expect(glider).toBeVisible();
	await expect(metric(glider, 'Pilot')).toHaveText('Demo Pilot');
	await expect(metric(glider, 'Position')).toHaveText(/^(Now|\d+ s ago)$/);
	await expect(metric(glider, 'Altitude')).toHaveText(/^1596\sm$/);
	await expect(metric(glider, 'Climb')).toHaveText(/^\u22121\.0\sm\/s$/);
	await expect(metric(glider, 'Speed')).toHaveText(/^38\skm\/h$/);
	await expect(metric(glider, 'Flying')).toHaveText(/^N\s20°$/);
	// The flight so far, once the server has handed its track out: twenty
	// minutes of it, with the climb the line on the map is red for.
	await expect(metric(glider, 'Shown for')).toHaveText(/^0:20\sh$/);
	await expect(metric(glider, 'Highest')).toHaveText(/^2117\sm$/);
	await expect(glider.getByRole('link', { name: 'This flight’s page on Venti' })).toHaveAttribute(
		'href',
		`/live/${FLYING.id}`
	);
	await expect(glider).toContainText('Positions are as each pilot’s app sent them.');
	// The link leads to the page the server makes for the track, in a tab of
	// its own: the page a pilot's link opens for someone without the app.
	const [serversPage] = await Promise.all([
		context.waitForEvent('page'),
		glider.getByRole('link', { name: 'This flight’s page on Venti' }).click()
	]);
	await expect(serversPage.getByRole('heading', { level: 1 })).toContainText('Demo Pilot');
	await serversPage.close();
	// The map moves once to show the whole flight beside the page.
	await mapDrawn();
	await keep('56-live-glider');

	// The flight was asked for whole, and then for what arrived since.
	const followed = () => answers.filter((answer) => answer.path.includes(FLYING.id));
	expect(followed()[0]).toEqual({ path: `/v1/live/${FLYING.id}`, status: 200 });
	await expect
		.poll(() => followed().some((answer) => answer.path.endsWith('?since=1201')), {
			timeout: 20_000
		})
		.toBe(true);

	await closeButton(glider, wide).click();
	await expect(glider).toBeHidden();
	expect(problems).toEqual([]);
});

test('the map can be kept on a glider in the air, and lets it go when it is moved by hand', async ({
	context,
	page,
	mapDrawn,
	keep,
	wide,
	colorScheme
}) => {
	test.skip(
		colorScheme === 'dark',
		'The appearance has no part in this; the light shows the card.'
	);
	/** Where the made-up next position is: the server's clock stands still, and its glider with it. */
	const NEXT = { lat: FLYING.latitude + 0.004, lon: FLYING.longitude + 0.012 };
	let moved = false;
	let track: Record<string, unknown> | undefined;
	await context.route(
		(address) => address.pathname === `/v1/live/${FLYING.id}`,
		async (route) => {
			if (!moved || !track) {
				const response = await route.fetch();
				if (response.status() === 200) track = (await response.json()).track;
				return route.fulfill({ response });
			}
			// Made up: the glider flew on to the east-north-east.
			const latest = {
				time: '2026-11-18T19:05:00Z',
				position: NEXT,
				altitude_m: 1610,
				ground_speed_ms: 10.5,
				climb_ms: 0.5,
				course_deg: 70.0
			};
			const since = new URL(route.request().url()).searchParams.get('since');
			return route.fulfill({
				json: {
					time: '2026-11-18T19:05:00Z',
					track: { ...track, latest, fixes: 1202 },
					fixes: since === '1201' ? [latest] : [],
					cursor: '1202',
					more: false,
					reset: false
				}
			});
		}
	);
	/** The middle of the map's visible part, and its zoom, as the map last came to rest. */
	const view = () =>
		page.evaluate(() => {
			const shown = (globalThis as { ventiView?: Record<string, number> }).ventiView!;
			return {
				zoom: shown.zoom,
				lat: (shown.south + shown.north) / 2,
				lon: (shown.west + shown.east) / 2
			};
		});

	await mapAt(context, page, mapDrawn, BETWEEN);
	await mapTool(page, 'Pilots in the air');
	await page
		.getByRole('dialog', { name: 'In the air' })
		.getByRole('listitem')
		.first()
		.getByRole('button')
		.click();
	const glider = gliderPanel(page, wide);
	await expect(metric(glider, 'Shown for')).toBeVisible();
	await mapDrawn();

	const onTheMap = glider.getByRole('button', { name: 'Follow on the map' });
	await expect(onTheMap).toHaveAttribute('aria-pressed', 'false');
	await onTheMap.click();
	await expect(onTheMap).toHaveAttribute('aria-pressed', 'true');
	await mapDrawn();
	await page.waitForTimeout(800);
	const before = await view();
	await keep('60b-live-follow-on-map');

	// The next position brings the map along, at the zoom it had.
	moved = true;
	await expect
		.poll(async () => (await view()).lon - before.lon, { timeout: 25_000 })
		.toBeCloseTo(NEXT.lon - FLYING.longitude, 3);
	const after = await view();
	expect(after.zoom).toBeCloseTo(before.zoom, 2);
	expect(after.lat - before.lat).toBeCloseTo(NEXT.lat - FLYING.latitude, 3);
	await expect(onTheMap).toHaveAttribute('aria-pressed', 'true');

	if (wide) {
		// Moved by hand, the map is the pilot's again.
		const map = (await page.locator('canvas.maplibregl-canvas').boundingBox())!;
		const from = { x: map.x + map.width * 0.7, y: map.y + map.height * 0.4 };
		await page.mouse.move(from.x, from.y);
		await page.mouse.down();
		await page.mouse.move(from.x - 60, from.y + 30, { steps: 6 });
		await page.mouse.up();
		await expect(onTheMap).toHaveAttribute('aria-pressed', 'false');
	} else {
		// A phone's sheet holds the map while it is open; closed, it lets the glider go.
		await closeButton(glider, wide).click();
		await expect(glider).toBeHidden();
		await mapTool(page, 'Pilots in the air');
		await page
			.getByRole('dialog', { name: 'In the air' })
			.getByRole('listitem')
			.first()
			.getByRole('button')
			.click();
		await expect(onTheMap).toHaveAttribute('aria-pressed', 'false');
	}
});

test('a position goes on ageing while the server has nothing new to say', async ({
	context,
	page,
	mapDrawn,
	colorScheme
}) => {
	test.skip(colorScheme === 'dark', 'The appearance has no part in this, and it takes a while.');
	const statuses: number[] = [];
	page.on('response', (response) => {
		if (new URL(response.url()).pathname === '/v1/live') statuses.push(response.status());
	});
	await mapAt(context, page, mapDrawn, BETWEEN);
	await mapTool(page, 'Pilots in the air');
	const flying = page.getByRole('dialog', { name: 'In the air' }).getByRole('listitem').first();

	// The server's clock stands still in its development mode, and its
	// gliders with it: asked again after a quarter of a minute it says "not
	// modified". The position was four seconds old when the server first
	// answered. Were "not modified" taken for a new answer, or handed to the
	// app as the kept answer with its old time, the age would start again
	// from four seconds at every question and never get past nineteen.
	await expect(flying).toContainText(/ · [2-9]\d s ago$/, { timeout: 40_000 });
	expect(statuses[0]).toBe(200);
	expect(statuses).toContain(304);
});

test('a tab that nobody looks at asks the server nothing, and catches up when it is looked at again', async ({
	context,
	page,
	mapDrawn,
	colorScheme
}) => {
	test.skip(colorScheme === 'dark', 'The appearance has no part in this, and it takes a while.');
	let asked = 0;
	page.on('request', (request) => {
		if (new URL(request.url()).pathname.startsWith('/v1/live')) asked += 1;
	});
	await mapAt(context, page, mapDrawn, BETWEEN);
	// A glider's page is open as well: its flight is asked for every ten seconds.
	await mapTool(page, 'Pilots in the air');
	const list = page.getByRole('dialog', { name: 'In the air' });
	await list.getByRole('listitem').first().getByRole('button').click();
	await expect(page.getByText('Shown for')).toBeVisible();

	/** Puts the tab behind another one, or in front again, as far as the page can tell. */
	const look = (state: 'hidden' | 'visible') =>
		page.evaluate((state) => {
			Object.defineProperty(document, 'visibilityState', { value: state, configurable: true });
			document.dispatchEvent(new Event('visibilitychange'));
		}, state);

	await look('hidden');
	// A question on its way when the tab went behind may still come back.
	await page.waitForTimeout(1000);
	const before = asked;
	// Longer than both rounds: a quarter of a minute for the map, ten
	// seconds for the flight.
	await page.waitForTimeout(17_000);
	expect(asked).toBe(before);

	await look('visible');
	// Both are asked at once, not at the next round.
	await expect.poll(() => asked, { timeout: 5000 }).toBeGreaterThanOrEqual(before + 2);
});

test('a glider is opened by its wing, and one on the ground says that it landed', async ({
	context,
	page,
	mapDrawn,
	keep,
	wide
}) => {
	// The pilot stands where the landed glider is, and the map goes there.
	// A phone's map is too narrow there for the one that still flies.
	await mapAt(context, page, mapDrawn, LANDED, wide ? 2 : 1);
	await clickTheMiddle(page);

	const glider = gliderPanel(page, wide);
	await expect(glider).toBeVisible();
	await expect(metric(glider, 'Position')).toHaveText(/^Landed (9|10) min ago$/);
	await expect(metric(glider, 'Altitude')).toHaveText(/^786\sm$/);
	// A glider on the ground neither climbs nor flies anywhere.
	await expect(glider.getByText('Climb')).toBeHidden();
	await expect(glider.getByText('Speed')).toBeHidden();
	await expect(glider.getByText('Flying')).toBeHidden();
	await expect(metric(glider, 'Shown for')).toHaveText(/^0:11\sh$/);
	await expect(glider.getByRole('link', { name: 'This flight’s page on Venti' })).toHaveAttribute(
		'href',
		`/live/${LANDED.id}`
	);
	await mapDrawn();
	await keep('57-live-landed');

	// A click on the bare map chooses nobody.
	await closeButton(glider, wide).click();
	const map = (await page.locator('canvas.maplibregl-canvas').boundingBox())!;
	await page.mouse.click(map.x + map.width / 2 + 150, map.y + map.height / 4);
	await expect(glider).toBeHidden();
});

test('a glider nothing was heard of for a while is drawn held back, and never called landed', async ({
	context,
	page,
	mapDrawn,
	keep,
	wide
}) => {
	// The flying glider's last position is seven minutes old: its phone has
	// no network, or its battery is empty. The server knows no more.
	const quiet = (track: { id: string; latest: { time: string } }) => {
		if (track.id === FLYING.id) track.latest.time = '2026-11-18T18:58:00Z';
	};
	await context.route('**/v1/live**', async (route) => {
		// Others in the air (`/v1/live/others`) are the gliding network's, not Venti's.
		if (new URL(route.request().url()).pathname === '/v1/live/others') return route.continue();
		const response = await route.fetch();
		if (response.status() !== 200) return route.fulfill({ response });
		const answer = await response.json();
		for (const track of answer.tracks ?? [answer.track]) quiet(track);
		await route.fulfill({ response, json: answer });
	});

	await mapAt(context, page, mapDrawn, BETWEEN);
	await mapTool(page, 'Pilots in the air');
	const list = page.getByRole('dialog', { name: 'In the air' });
	await expect(list.getByRole('listitem').first()).toContainText(
		/1596 m · \u22121\.0 m\/s · No position for 7 min$/
	);
	await list.getByRole('listitem').first().getByRole('button').click();

	const glider = gliderPanel(page, wide);
	await expect(metric(glider, 'Position')).toHaveText('No position for 7 min');
	await expect(glider).not.toContainText('Landed');
	await mapDrawn();
	await keep('58-live-quiet');
});

test('a flight its pilot stopped showing is said to be gone, in the core’s words', async ({
	context,
	page,
	mapDrawn,
	keep,
	wide
}) => {
	let shownToEveryone = true;
	await context.route('**/v1/live**', async (route) => {
		if (shownToEveryone) return route.continue();
		// The pilot chose "Nobody": the track is deleted, and no list has it.
		if (new URL(route.request().url()).pathname === '/v1/live') {
			return route.fulfill({
				json: { time: '2026-11-18T19:05:00Z', tracks: [], truncated: false }
			});
		}
		return route.fulfill({
			status: 404,
			json: { error: { code: 'live_track_not_found', message: 'No such live track.' } }
		});
	});

	await mapAt(context, page, mapDrawn, BETWEEN);
	await mapTool(page, 'Pilots in the air');
	const list = page.getByRole('dialog', { name: 'In the air' });
	await list.getByRole('listitem').first().getByRole('button').click();
	const glider = gliderPanel(page, wide);
	await expect(metric(glider, 'Shown for')).toBeVisible();

	shownToEveryone = false;
	// Within a quarter of a minute both the flight and the map are asked again.
	const gone = page.getByRole(wide ? 'complementary' : 'dialog', { name: 'Glider' });
	await expect(gone).toContainText(
		'This flight is no longer shown: its pilot stopped sharing it, or it is more than two days old.',
		{ timeout: 40_000 }
	);
	await expect(shown(page)).toHaveText('In the air on this part of the map: 0.');
	await mapDrawn();
	await keep('59-live-gone');
});

test('a map of half a continent asks for nobody, and the list says how to get someone', async ({
	page,
	mapDrawn,
	keep,
	wide
}) => {
	await page.goto('/map/');
	await mapTabDrawn(page, mapDrawn);
	// The northern Alps from Lake Constance to Salzburg: gliders are asked for.
	await expect(shown(page)).toBeVisible();

	// Further out, with the keyboard, until the map is wider than six
	// degrees. A phone's map is narrow and needs more steps.
	const map = page.locator('canvas.maplibregl-canvas');
	await map.focus();
	for (let step = 0; step < (wide ? 1 : 3); step += 1) {
		await page.keyboard.press('Minus');
		await mapDrawn();
	}
	await map.blur();
	await expect(shown(page)).toBeHidden();

	await mapTool(page, 'Pilots in the air');
	const list = page.getByRole('dialog', { name: 'In the air' });
	await expect(list).toContainText('The map shows too much');
	await expect(list).toContainText(
		'Zoom the map in on an area, and the pilots who show their flight to everyone there are listed.'
	);
	await keep('60-live-zoomed-out');
});
