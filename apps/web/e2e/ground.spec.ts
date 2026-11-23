/**
 * The ground under a flight's altitude profile: the earth as a filled
 * shape along the foot of the chart, and the height over it at the marked
 * moment.
 *
 * The app fetches the elevation tiles under a flight from the tile server
 * its map draws its relief from, and the core reads the heights. Here the
 * test browser answers those requests with tiles cut from the region in
 * the repository, Mapterhorn's elevation around the Tegernsee, where the
 * sample flight is: real tiles, on the way the app takes them, without the
 * network.
 */

import { readFileSync } from 'node:fs';
import {
	addSampleFlight,
	asUpload,
	expect,
	groundDrawn,
	metric,
	panel,
	SAMPLE_FLIGHT,
	signUp,
	test
} from './helpers';

/** The newest of the development mode's public flights, Jonas Beispiel's. */
const ANOTHER_PILOTS_FLIGHT = 'nfmvgygyr34epnfbcekb04nwqc';

test('a flight’s profile is drawn over the ground, and the marked moment says how high over it', async ({
	page,
	mapDrawn,
	keep,
	problems
}) => {
	const { moment } = panel(page);
	const chart = page.getByRole('slider', { name: 'Altitude over time' });
	// Only the tiles under the flight are asked for, and of the map's own
	// tile server: the sample flight lies on two tiles of zoom 12.
	const fetched: string[] = [];
	page.on('request', (request) => {
		if (request.url().startsWith('https://tiles.mapterhorn.com/12/')) fetched.push(request.url());
	});

	await addSampleFlight(page);

	// In words, for those who do not see the picture: the valley floor of
	// the Tegernsee and the Wallberg's ridge, as the tiles have them.
	await expect(chart).toHaveAttribute(
		'aria-valuetext',
		'From 786 to 2117 metres, over ground from 731 to 1710 metres. Nothing marked.'
	);
	for (const tile of ['12/2181/1429', '12/2182/1429']) {
		expect(fetched).toContain(`https://tiles.mapterhorn.com/${tile}.webp`);
	}
	await mapDrawn();
	await keep('71-flight-ground');

	// The end of the flight: 53 m over the valley, where the time in the
	// air stood before the ground was known.
	await chart.focus();
	await page.keyboard.press('End');
	await expect(metric(moment, 'Time')).toHaveText('12:59');
	await expect(metric(moment, 'Over ground')).toHaveText('53 m');
	await expect(metric(moment, 'In the air')).toBeHidden();
	await expect(chart).toHaveAttribute(
		'aria-valuetext',
		/^12:59, 786 metres, 53 metres over the ground, /
	);

	// The take-off: on the ground. The made-up flight starts a little under
	// the model's ground, which is not a height below it.
	await page.keyboard.press('Home');
	await expect(metric(moment, 'Over ground')).toHaveText('0 m');

	// Half-way, high over the lake.
	await page.keyboard.press('PageUp');
	await page.keyboard.press('PageUp');
	await page.keyboard.press('PageUp');
	await page.keyboard.press('PageUp');
	await page.keyboard.press('PageUp');
	await expect(metric(moment, 'Over ground')).toHaveText(/^1\d{3}\sm$/);
	await chart.blur();
	await keep('72-flight-ground-moment');

	expect(problems).toEqual([]);
});

test('another pilot’s flight has the ground under its profile too', async ({ page }) => {
	await page.goto(`/flights/${ANOTHER_PILOTS_FLIGHT}/`);
	await expect(page.getByRole('link', { name: 'Jonas Beispiel' })).toBeVisible();

	await groundDrawn(page);

	const chart = page.getByRole('slider', { name: 'Altitude over time' });
	await chart.focus();
	await page.keyboard.press('End');
	await expect(metric(panel(page).moment, 'Over ground')).toHaveText(/^[\d,]+\sm$/);
});

test('without elevation tiles the profile is as it was', async ({ context, page, keep }) => {
	// The tile server has nothing for this place: what it says of a tile
	// that does not exist.
	await context.route('https://tiles.mapterhorn.com/**', (route) =>
		route.fulfill({ status: 404, headers: { 'access-control-allow-origin': '*' } })
	);
	const { moment } = panel(page);
	const chart = page.getByRole('slider', { name: 'Altitude over time' });
	const asked = page.waitForResponse('https://tiles.mapterhorn.com/12/2182/1429.webp');

	await signUp(page);
	await page.goto('/settings/account/flights/');
	await page
		.locator('input[type=file]')
		.setInputFiles(asUpload('sample.igc', readFileSync(SAMPLE_FLIGHT, 'utf8')));
	await expect(page.getByRole('heading', { name: '4 Jul 2026' })).toBeVisible();
	await asked;

	await expect(chart).toHaveAttribute('aria-valuetext', 'From 786 to 2117 metres. Nothing marked.');
	await chart.focus();
	await page.keyboard.press('End');
	await expect(metric(moment, 'In the air')).toHaveText('0:29 h');
	await expect(metric(moment, 'Over ground')).toBeHidden();
	await chart.blur();
	await keep('73-flight-no-ground');
});
