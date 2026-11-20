/**
 * Live wind on the map, from the Venti server in its development mode: six
 * stations as they were on 18 November 2026 at 19:05 UTC, with the server's
 * clock standing still at that moment. `playwright.config.ts` starts it.
 */

import type { Locator, Page } from '@playwright/test';
import { expect, mapTabDrawn, metric, showMyPosition, test } from './helpers';

/** Where the weather station of Garmisch-Partenkirchen stands. */
const GARMISCH = { latitude: 47.483, longitude: 11.0621 };

/** A chosen station: a sheet on a narrow screen, a card beside the map on a wide one. */
function stationPanel(page: Page, wide: boolean, name: string): Locator {
	return page.getByRole(wide ? 'complementary' : 'dialog', { name });
}

const closeButton = (panel: Locator, wide: boolean) =>
	panel.getByRole('button', { name: wide ? 'Close' : 'Done' });

test('the map shows the wind, lists its stations and opens one', async ({
	page,
	mapDrawn,
	keep,
	wide,
	problems
}) => {
	await page.goto('/');
	await mapTabDrawn(page, mapDrawn);

	// Whose data is on the map is written on the map.
	await expect(page.getByText('Wind: Deutscher Wetterdienst and GeoSphere Austria')).toBeVisible();
	// The map places the arrows over a few frames and fades them in.
	await page.waitForTimeout(1500);
	await keep('18-map-wind');

	// Everything the arrows say, as a list by name.
	await page.getByRole('button', { name: 'Wind stations' }).click();
	const list = page.getByRole('dialog', { name: 'Wind stations' });
	const rows = list.getByRole('listitem');
	// The stations around what the map shows. A phone's narrow map leaves
	// out the Sonnblick, far in the east.
	await expect(rows).toHaveCount(wide ? 6 : 5);
	await expect(rows.first()).toContainText('Garmisch-Partenkirchen');
	await expect(rows.first()).toContainText('719 m');
	await expect(rows.first()).toContainText('5/8 km/h');
	// A name the provider shouts is set in ordinary case, by the core.
	await expect(rows.nth(3)).toContainText('Patscherkofel');
	// A screen reader hears a sentence, not a fraction.
	await expect(
		list.getByRole('button', {
			name: 'Zugspitze. Wind 54 kilometres per hour, gusts 93, from W. 2956 m'
		})
	).toBeVisible();
	await expect(list).toContainText('Quelle: Deutscher Wetterdienst');
	await expect(list).toContainText('Data source: GeoSphere Austria');
	await keep('19-wind-stations');

	// Looked up by a part of its name.
	await list.getByRole('searchbox', { name: 'Station' }).fill('nothing like this');
	await expect(list).toContainText('No results for “nothing like this”');
	await list.getByRole('searchbox', { name: 'Station' }).fill('SPITZ');
	await expect(rows).toHaveCount(1);
	await rows.first().getByRole('button').click();
	await expect(list).toBeHidden();

	// The station: what it measures now, by the server's clock.
	const station = stationPanel(page, wide, 'Zugspitze');
	await expect(station).toBeVisible();
	await expect(metric(station, 'Wind')).toHaveText(/^54\skm\/h$/);
	await expect(metric(station, 'Gusts')).toHaveText(/^93\skm\/h$/);
	await expect(metric(station, 'From')).toHaveText(/^W\s260°$/);
	await expect(metric(station, 'Temperature')).toHaveText(/^-7\s°C$/);
	await expect(metric(station, 'Height')).toHaveText(/^2956\sm$/);
	await expect(metric(station, 'Measured')).toHaveText('19:20');
	await expect(station).toContainText('Measured 45 minutes ago.');

	// Its last 24 hours, and whose data it is.
	const chart = station.getByRole('slider', {
		name: 'Wind and gusts over the last 24 hours, in kilometres per hour'
	});
	await expect(chart).toBeVisible();
	await expect(
		station.getByRole('img', { name: 'Wind direction over the last 24 hours' })
	).toBeVisible();
	await expect(station).toContainText('Wind and gusts in km/h, and where the wind came from.');
	await expect(station).toContainText('Quelle: Deutscher Wetterdienst');
	await expect(station).toContainText('Deutscher Wetterdienst, under CC BY 4.0.');
	await expect(station.getByRole('link', { name: 'www.dwd.de' })).toHaveAttribute(
		'href',
		'https://www.dwd.de/DE/service/rechtliche_hinweise/rechtliche_hinweise_node.html'
	);
	await keep('20-wind-station');

	// The keyboard reads the chart one reading at a time, newest first.
	await chart.focus();
	await page.keyboard.press('ArrowLeft');
	await expect(chart).toHaveAttribute('aria-valuetext', '19:20 · 54 km/h · gusts 93 · from W');
	await page.keyboard.press('ArrowLeft');
	await expect(chart).toHaveAttribute('aria-valuetext', '19:10 · 49 km/h · gusts 101 · from W');
	await expect(station).toContainText('19:10 · 49 km/h · gusts 101 · from W');
	await page.keyboard.press('PageDown');
	await expect(chart).toHaveAttribute('aria-valuetext', /^18:10 · /);
	await chart.scrollIntoViewIfNeeded();
	await keep('21-wind-station-reading');
	await page.keyboard.press('Escape');
	await expect(chart).toHaveAttribute('aria-valuetext', 'Nothing marked.');

	// A mouse reads the chart by passing over it, and lets go by leaving.
	if (wide) {
		const plot = (await chart.boundingBox())!;
		await page.mouse.move(plot.x + plot.width / 2, plot.y + plot.height / 2, { steps: 3 });
		await expect(chart).toHaveAttribute(
			'aria-valuetext',
			/^\d+:\d\d · \d+ km\/h · gusts \d+ · from /
		);
		await page.mouse.move(plot.x + plot.width / 2, plot.y - 60, { steps: 3 });
		await expect(chart).toHaveAttribute('aria-valuetext', 'Nothing marked.');
	}

	await closeButton(station, wide).click();
	await expect(station).toBeHidden();
	expect(problems).toEqual([]);
});

test('a station is opened by its arrow, also one that gave way to a windier neighbour', async ({
	context,
	page,
	mapDrawn,
	wide
}) => {
	// From afar Garmisch gives way to the Zugspitze, nine kilometres off and
	// far windier. The pilot stands at the station, and the map goes there.
	await context.grantPermissions(['geolocation']);
	await context.setGeolocation(GARMISCH);
	await page.goto('/');
	await mapTabDrawn(page, mapDrawn);
	await showMyPosition(page, mapDrawn);
	await page.waitForTimeout(800);

	// The station is in the middle of what the navigation leaves of the map.
	const map = (await page.locator('canvas.maplibregl-canvas').boundingBox())!;
	const covered = await page.locator('.navigation-height').evaluate((bar) => bar.clientHeight);
	// A little beside the arrow: a finger is not that exact.
	await page.mouse.click(map.x + map.width / 2 + 12, map.y + (map.height - covered) / 2 - 10);

	const station = stationPanel(page, wide, 'Garmisch-Partenkirchen');
	await expect(station).toBeVisible();
	await expect(metric(station, 'Wind')).toHaveText(/^5\skm\/h$/);
	await expect(metric(station, 'From')).toHaveText(/^N\s350°$/);
	// This station has no thermometer.
	await expect(metric(station, 'Temperature')).toHaveText(/^–\s°C$/);

	// A click on the bare map chooses nothing.
	await closeButton(station, wide).click();
	await page.mouse.click(map.x + map.width / 2 + 150, map.y + map.height / 4);
	await expect(station).toBeHidden();
});

test('the map says so in words when the server cannot be reached', async ({
	context,
	page,
	keep
}) => {
	await context.route('**/v1/**', (route) => route.abort());
	await page.goto('/');

	await expect(page.getByRole('status')).toHaveText('The wind server could not be reached.');
	await expect(page.getByText(/^Wind: /)).toBeHidden();
	await keep('22-wind-unreachable');

	// The list says the same instead of being empty without a reason.
	await page.getByRole('button', { name: 'Wind stations' }).click();
	const list = page.getByRole('dialog', { name: 'Wind stations' });
	await expect(list).toContainText('No stations here');
	await expect(list).toContainText('The wind server could not be reached.');
});

test('a server in trouble is reported with what it answered', async ({ context, page }) => {
	await context.route('**/v1/**', (route) => route.fulfill({ status: 502, body: 'Bad Gateway' }));
	await page.goto('/');

	await expect(page.getByRole('status')).toHaveText('The server could not answer (status 502).');
});

test('a site without a server has a map without wind and without sites, and says nothing', async ({
	context,
	page,
	mapDrawn
}) => {
	// What a web server answers that only hands out the app's files.
	await context.route('**/v1/**', (route) => route.fulfill({ status: 404, body: 'Not found' }));
	await page.goto('/');
	await mapDrawn();

	await expect(page.getByRole('button', { name: 'Show my position' })).toBeVisible();
	await expect(page.getByRole('button', { name: 'Wind stations' })).toBeHidden();
	await expect(page.getByRole('button', { name: 'Flying sites' })).toBeHidden();
	await expect(page.getByRole('status')).toBeHidden();
	await expect(page.getByText(/^Wind: /)).toBeHidden();
});
