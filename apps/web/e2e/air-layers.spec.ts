/**
 * What many gliders say together: "Climbs now" and "Where thermals have
 * been", two rows of the layers' sheet that are off until switched on; the
 * line of who flies at a take-off; and an alarm that waits for pilots in
 * the air at a place.
 *
 * The development server has made-up gliders above the Tegernsee: three
 * that climbed in one thermal east of the Wallberg (+2.4 m/s, four minutes
 * ago by its clock), two that climb south of the lake now, and two cells
 * in which its made-up public flights climbed. None of it is a real
 * pilot's.
 */

import type { Page } from '@playwright/test';
import {
	BETWEEN_THE_MOUNTAINS,
	closeLayers,
	expect,
	layerSwitch,
	mapTabDrawn,
	mapTool,
	metric,
	openCredits,
	openLayers,
	showTheValley,
	switchLayer,
	test
} from './helpers';

interface Drawn {
	climbs: string[];
	flown: number;
	climbOnScreen: (index: number) => { x: number; y: number } | undefined;
	flownOnScreen: (index: number) => { x: number; y: number } | undefined;
}

/** What the map draws of the two layers, as the page says it. */
const drawn = (page: Page) =>
	page.evaluate(() => {
		const air = (globalThis as { ventiAirLayers?: Drawn }).ventiAirLayers;
		return { climbs: air?.climbs ?? [], flown: air?.flown ?? 0 };
	});

/** Where a mark of one of the layers stands in the page, for a click. */
async function markAt(page: Page, layer: 'climb' | 'flown', index: number) {
	const map = (await page.locator('canvas.maplibregl-canvas').boundingBox())!;
	const at = await page.evaluate(
		([which, nth]) => {
			const air = (globalThis as unknown as { ventiAirLayers: Drawn }).ventiAirLayers;
			return which === 'climb'
				? air.climbOnScreen(nth as number)
				: air.flownOnScreen(nth as number);
		},
		[layer, index] as const
	);
	return { x: map.x + at!.x, y: map.y + at!.y };
}

/** The wind, the sites and the pilots off, so that a click finds the layer's own marks. */
const onlyTheAir = (page: Page) =>
	page.addInitScript(() => {
		// Nothing shown over the ground: no wind's arrows either.
		localStorage.setItem('venti.map-what', 'none');
		localStorage.setItem('venti.map-layers-off', JSON.stringify(['sites', 'pilots']));
	});

async function signUp(page: Page): Promise<void> {
	const handle = `pilot${Math.random().toString(36).slice(2, 10)}`;
	const field = (name: string) => page.getByRole('textbox', { name, exact: true });
	await page.goto('/settings/account/');
	await page.getByRole('radio', { name: 'New account' }).check();
	await field('Handle').fill(handle);
	await field('Name shown to others').fill('Test Pilot');
	await field('Password').fill('correct horse battery staple');
	await page.getByRole('button', { name: 'Create account' }).click();
	await expect(metric(page, 'Handle')).toHaveText(`@${handle}`);
}

test('“Climbs now” marks where pilots climb, and a click reads the core’s line', async ({
	context,
	page,
	mapDrawn,
	keep,
	wide,
	problems
}) => {
	await onlyTheAir(page);
	await context.grantPermissions(['geolocation']);
	await context.setGeolocation(BETWEEN_THE_MOUNTAINS);
	await page.goto('/map/');
	await mapTabDrawn(page, mapDrawn);
	await showTheValley(page, mapDrawn, wide ? 1 : 2);

	// Off until the pilot switches it on: nothing is drawn.
	expect((await drawn(page)).climbs).toEqual([]);
	const sheet = await openLayers(page);
	await expect(layerSwitch(page, 'Climbs now')).not.toBeChecked();
	await layerSwitch(page, 'Climbs now').check();
	// Under its row, the core's count.
	await expect(sheet).toContainText('Where gliders circled and climbed in the last 10 minutes.');
	await expect(sheet).toContainText(/\d+ climbs? in the last 10 minutes/);
	await keep('260-map-layers-climbs');
	await closeLayers(page);

	// A mark for each climb, with the core's label on it.
	await expect.poll(async () => (await drawn(page)).climbs).toContain('+2.4');
	await mapDrawn();
	await page.waitForTimeout(800);
	await keep('261-map-climbs');

	// A click on the climb east of the Wallberg: its line, in the card of its place.
	const index = (await drawn(page)).climbs.indexOf('+2.4');
	const at = await markAt(page, 'climb', index);
	await page.mouse.click(at.x, at.y);
	const card = page.getByRole(wide ? 'complementary' : 'dialog', { name: /° N, / });
	await expect(card).toContainText(
		/Climbing now: \+2\.4 m\/s between 1,600 and 2,100 m, 3 pilots, \d+ min ago/
	);
	await expect(card.getByRole('button', { name: 'What a glide reaches from here' })).toBeVisible();
	await keep('262-climb');
	await card.getByRole('button', { name: wide ? 'Close' : 'Done' }).click();

	// Whose positions the climbs are read from is in the credits.
	await openCredits(page);
	await expect(page.locator('.credits')).toContainText(
		'Positions: Open Glider Network, glidernet.org (ODbL)'
	);

	// Kept by the browser, and off again: nothing drawn.
	await page.reload();
	await mapTabDrawn(page, mapDrawn);
	await expect.poll(async () => (await drawn(page)).climbs.length).toBeGreaterThan(0);
	await switchLayer(page, 'Climbs now', false);
	await expect.poll(async () => (await drawn(page)).climbs).toEqual([]);
	expect(problems).toEqual([]);
});

test('“Where thermals have been” marks the cells public flights climbed in, with what the layer is under its row', async ({
	context,
	page,
	mapDrawn,
	keep,
	wide,
	problems
}) => {
	await onlyTheAir(page);
	await context.grantPermissions(['geolocation']);
	await context.setGeolocation(BETWEEN_THE_MOUNTAINS);
	await page.goto('/map/');
	await mapTabDrawn(page, mapDrawn);

	// The whole northern Alps is too much to ask for: the core says so.
	const sheet = await openLayers(page);
	await layerSwitch(page, 'Where thermals have been').check();
	await expect(sheet).toContainText(
		'From flights their pilots shared, in every month of the year.'
	);
	await expect(sheet).toContainText('Zoom in to see where pilots climbed.');
	await closeLayers(page);

	// Two cells above the valley; a phone's map is narrower and shows one of them.
	await showTheValley(page, mapDrawn, wide ? 1 : 2);
	await expect.poll(async () => (await drawn(page)).flown).toBeGreaterThan(0);
	await mapDrawn();
	await page.waitForTimeout(800);
	await keep('264-map-flown-thermals');

	// What the layer is under its row, and its terms behind the core's link.
	await openLayers(page);
	await expect(sheet).toContainText(/Where pilots climbed, from the public flights on Venti/);
	await expect(sheet.getByText(/^These flights are their pilots'/)).toBeHidden();
	await sheet.getByText('Terms for these flights').click();
	await expect(sheet.getByText(/^These flights are their pilots'/)).toBeVisible();
	await keep('265-map-layers-flown');
	await closeLayers(page);

	// A click on a cell: the core's line about it.
	const at = await markAt(page, 'flown', 0);
	await page.mouse.click(at.x, at.y);
	const card = page.getByRole(wide ? 'complementary' : 'dialog', { name: /° N, / });
	await expect(card).toContainText(/\d+ climbs of 3 pilots/);
	expect(problems).toEqual([]);
});

test('a take-off says who flies there now, and an alarm waits for pilots in the air there', async ({
	context,
	page,
	mapDrawn,
	keep,
	wide,
	problems
}) => {
	await signUp(page);
	await context.grantPermissions(['geolocation']);
	await context.setGeolocation(BETWEEN_THE_MOUNTAINS);
	await page.goto('/map/');
	await mapTabDrawn(page, mapDrawn);
	await showTheValley(page, mapDrawn, wide ? 1 : 2);

	// One of the Wallberg's ramps, which their source does not name.
	await mapTool(page, 'Flying sites');
	await page
		.getByRole('dialog', { name: 'Flying sites' })
		.getByRole('listitem')
		.filter({ hasText: /Take-off\s*No more is known/ })
		.first()
		.getByRole('button')
		.click();
	const site = page.getByRole(wide ? 'complementary' : 'dialog').filter({
		has: page.locator('[data-site-air]')
	});
	await expect(site.locator('[data-site-air]')).toHaveText(
		/^\d+ pilots in the air here, \d+ took off in the last hour$/
	);
	await keep('263-site-in-the-air');

	// "Alert me": a new alarm for a place offers its two kinds.
	await site.getByRole('button', { name: 'Alert me' }).click();
	const form = page.getByRole('dialog', { name: 'Alert me' });
	const kinds = form.getByRole('group', { name: 'Kind' });
	await expect(kinds.getByRole('button', { name: 'Flying day' })).toHaveAttribute(
		'aria-pressed',
		'true'
	);
	await kinds.getByRole('button', { name: 'Pilots in the air' }).click();
	const line = form.locator('[data-alert-line]');
	await expect(line).toHaveText(/^3 pilots or more in the air at .+, at most once in 6 hours$/);
	await form.getByRole('textbox', { name: 'Pilots in the air, at least' }).fill('5');
	await form.getByRole('textbox', { name: 'Then quiet for, hours' }).fill('3');
	await expect(line).toHaveText(/^5 pilots or more in the air at .+, at most once in 3 hours$/);
	await keep('266-alert-pilots');
	await form.getByRole('button', { name: 'Save' }).click();
	await expect(form).toBeHidden();

	// Listed with the core's line.
	await page.goto('/settings/account/alerts/');
	const list = page.getByRole('list', { name: 'Your alarms' });
	await expect(list.getByRole('listitem')).toHaveCount(1);
	await expect(list).toContainText(/5 pilots or more in the air at .+, at most once in 3 hours/);
	expect(problems).toEqual([]);
});
