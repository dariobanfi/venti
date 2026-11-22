/**
 * Live wind on the map, from the Venti server in its development mode: six
 * stations as they were on 18 November 2026 at 19:05 UTC, with the server's
 * clock standing still at that moment. `playwright.config.ts` starts it.
 */

import type { Locator, Page } from '@playwright/test';
import {
	expect,
	mapTabDrawn,
	metric,
	showMyPosition,
	test,
	mapTool,
	openCredits,
	openLayers,
	closeLayers,
	layerSwitch,
	toolRow
} from './helpers';

/** Where the weather station of Garmisch-Partenkirchen stands. */
const GARMISCH = { latitude: 47.483, longitude: 11.0621 };
/** Between MeteoSwiss's and the SLF's stations on the Weissfluhjoch, 400 m apart. */
const WEISSFLUHJOCH = { latitude: 46.8315, longitude: 9.8078 };
/** Where Pioupiou's sensor at the Osterfelder take-off stands. */
const OSTERFELDER = { latitude: 47.439445, longitude: 11.052041 };

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
	await page.goto('/map/');
	await mapTabDrawn(page, mapDrawn);

	// Whose data is on the map is written on the map.
	await openCredits(page);
	await expect(page.getByText(/^Wind: .*Deutscher Wetterdienst.*GeoSphere Austria/)).toBeVisible();
	// The map places the arrows over a few frames and fades them in.
	await page.waitForTimeout(1500);
	await keep('18-map-wind');

	// Everything the arrows say, as a list by name.
	await mapTool(page, 'Wind stations');
	const list = page.getByRole('dialog', { name: 'Wind stations' });
	const rows = list.getByRole('listitem');
	// The stations around what the map shows, each by its name.
	const garmisch = rows.filter({ hasText: 'Garmisch-Partenkirchen' });
	await expect(garmisch).toContainText('719 m');
	await expect(garmisch).toContainText('5/8 km/h');
	// A name the provider shouts is set in ordinary case, by the core.
	await expect(rows.filter({ hasText: 'Patscherkofel' })).toHaveCount(1);
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
	await expect(rows.filter({ hasText: 'Zugspitze' })).toHaveCount(1);
	await expect(rows.filter({ hasText: 'Garmisch-Partenkirchen' })).toHaveCount(0);
	await rows.filter({ hasText: 'Zugspitze' }).getByRole('button').click();
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

test('a station’s page says what its readings tell of the air, and shows a past day', async ({
	page,
	mapDrawn,
	keep,
	wide,
	problems
}) => {
	await page.goto('/map/');
	await mapTabDrawn(page, mapDrawn);
	await mapTool(page, 'Wind stations');
	const list = page.getByRole('dialog', { name: 'Wind stations' });
	await list.getByRole('searchbox', { name: 'Station' }).fill('Innsbruck');
	await list
		.getByRole('listitem')
		.filter({ hasText: 'Innsbruck/Universitaet' })
		.getByRole('button')
		.click();
	const station = stationPanel(page, wide, 'Innsbruck/Universitaet');

	// In the valley, with the Patscherkofel 1673 m above it 8 km away. The
	// server's clock stands after dark, when nothing rises from a station:
	// no cloud base, none of the six stations gives one then.
	// Behind a row at the end, closed from the start, as on the phones.
	const air = station.getByRole('group', { name: 'More about the air here' });
	await expect(air.getByText('Cooler above, as in air at rest')).toBeHidden();
	await air.scrollIntoViewIfNeeded();
	await keep('221-wind-station-air-folded');
	await air.locator('summary').click();
	await expect(air.getByText('Cooler above, as in air at rest')).toBeVisible();
	await expect(air).toContainText('Patscherkofel, 8.2 km away and 1673 m higher');
	await expect(air).toContainText('−0.60 °C per 100 m');
	await expect(air).toContainText('Cooler above, as in air at rest');
	await expect(air).toContainText('The standard atmosphere has 0.65 °C.');
	await expect(air).toContainText('not a sounding');
	await expect(air).not.toContainText('cloud base');
	await air.scrollIntoViewIfNeeded();
	await keep('116-wind-station-air');

	// A station with a recorded history: today first, a year back at most.
	await closeButton(station, wide).click();
	await mapTool(page, 'Wind stations');
	await list.getByRole('searchbox', { name: 'Station' }).fill('Zugspitze');
	await list.getByRole('listitem').first().getByRole('button').click();
	const summit = stationPanel(page, wide, 'Zugspitze');
	const day = summit.getByLabel('Day', { exact: true });
	await expect(day).toHaveValue('2026-11-18');
	await expect(day).toHaveAttribute('min', '2025-11-18');
	await expect(summit).toContainText('shown for the last year');
	await expect(summit.getByRole('slider', { name: /over the last 24 hours/ })).toBeVisible();

	// A day the server has nothing of says so in words: the development
	// server keeps no readings of the days before its clock.
	await day.fill('2026-11-17');
	await expect(summit).toContainText('The server has no readings of this station for this day.');
	await expect(summit.getByRole('heading', { name: /^Tue,? 17 Nov/ })).toBeVisible();
	await day.scrollIntoViewIfNeeded();
	await keep('117-wind-station-day-empty');

	// A day it has readings of is drawn from midnight to midnight. MADE UP:
	// the development server has readings of its own day only, so the
	// question for the 17th is answered with the recorded readings of the
	// 18th, moved back a day.
	await page.route('**/v1/stations/*/observations?*', async (route) => {
		const asked = new URL(route.request().url());
		const response = await route.fetch({ url: `${asked.origin}${asked.pathname}` });
		const body = (await response.text()).replaceAll('2026-11-18T', '2026-11-17T');
		await route.fulfill({ response, body });
	});
	await day.fill('2026-11-16');
	await day.fill('2026-11-17');
	await expect(
		summit.getByRole('slider', { name: /^Wind and gusts on Tue,? 17 Nov/ })
	).toBeVisible();
	await expect(summit.getByRole('heading', { name: /^Tue,? 17 Nov/ })).toBeVisible();
	await day.scrollIntoViewIfNeeded();
	await keep('117-wind-station-day');

	// Back to today: the last 24 hours.
	await day.fill('2026-11-18');
	await expect(summit.getByRole('slider', { name: /over the last 24 hours/ })).toBeVisible();
	expect(problems).toEqual([]);
});

test('two stations of one name are told apart in the list', async ({
	context,
	page,
	mapDrawn,
	keep
}) => {
	// The list holds the stations around the map in view: the pilot stands
	// on the Weissfluhjoch.
	await context.grantPermissions(['geolocation']);
	await context.setGeolocation(WEISSFLUHJOCH);
	await page.goto('/map/');
	await mapTabDrawn(page, mapDrawn);
	await showMyPosition(page, mapDrawn);
	await mapTool(page, 'Wind stations');
	const list = page.getByRole('dialog', { name: 'Wind stations' });
	const rows = list.getByRole('listitem');

	// Two real stations on the Weissfluhjoch, MeteoSwiss's and the SLF's, at
	// different heights: the core's line under each name says its height
	// and its network.
	await list.getByRole('searchbox', { name: 'Station' }).fill('Weissfluhjoch');
	await expect(rows).toHaveCount(2);
	await expect(rows.filter({ hasText: /2691 m · (.+ · )?MeteoSwiss/ })).toHaveCount(1);
	await expect(rows.filter({ hasText: /2536 m · (.+ · )?SLF/ })).toHaveCount(1);
	await keep('19b-wind-stations-same-name');
});

test('a station its network gives no height has none', async ({
	context,
	page,
	mapDrawn,
	keep,
	wide,
	problems
}) => {
	// A Pioupiou sensor by the Osterfelder take-off: its network gives
	// neither a height nor a temperature.
	await context.grantPermissions(['geolocation']);
	await context.setGeolocation(OSTERFELDER);
	await page.goto('/map/');
	await mapTabDrawn(page, mapDrawn);
	await showMyPosition(page, mapDrawn);
	await mapTool(page, 'Wind stations');
	const list = page.getByRole('dialog', { name: 'Wind stations' });
	await list.getByRole('searchbox', { name: 'Station' }).fill('Osterfelder');
	const row = list.getByRole('listitem').filter({ hasText: 'Osterfelder' });
	await expect(row).toHaveCount(1);
	await expect(row).not.toContainText(/\d\s?m\b(?!\/)/);
	await row.getByRole('button').click();
	const station = stationPanel(page, wide, 'Osterfelder Nord -Startplatz');
	await expect(station).toBeVisible();
	// No height at all: no "0 m", and no dash with a unit.
	await expect(station.locator('dt', { hasText: /^Height$/ })).toHaveCount(0);
	await expect(station).not.toContainText(/\b0\sm\b/);
	await expect(station).not.toContainText(/–\s(m|°C)\b/);
	await expect(station).not.toContainText(/higher|lower/);
	await expect(station).toContainText('OpenWindMap');
	await keep('20b-wind-station-no-height');
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
	await page.goto('/map/');
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
	// This station has no thermometer: a dash, and no unit that reads as a value.
	await expect(metric(station, 'Temperature')).toHaveText(/^–$/);

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
	await page.goto('/map/');

	await expect(page.locator('.notice')).toHaveText('Venti could not be reached for the wind.');
	await expect(page.getByText(/^Wind: /)).toBeHidden();
	await keep('22-wind-unreachable');

	// The list says the same instead of being empty without a reason.
	await mapTool(page, 'Wind stations');
	const list = page.getByRole('dialog', { name: 'Wind stations' });
	await expect(list).toContainText('No stations here');
	await expect(list).toContainText('Venti could not be reached for the wind.');
});

test('a server in trouble is reported with what it answered', async ({ context, page }) => {
	await context.route('**/v1/**', (route) => route.fulfill({ status: 502, body: 'Bad Gateway' }));
	await page.goto('/map/');

	await expect(page.locator('.notice')).toHaveText('The server could not answer (status 502).');
});

test('a site without a server has a map without wind and without sites, and says nothing', async ({
	context,
	page,
	mapDrawn
}) => {
	// What a web server answers that only hands out the app's files.
	await context.route('**/v1/**', (route) => route.fulfill({ status: 404, body: 'Not found' }));
	await page.goto('/map/');
	await mapDrawn();

	await expect(page.getByRole('button', { name: 'Show my position' })).toBeVisible();
	// The layers' sheet has the ground and the route planner, and no list of
	// what a server would have answered.
	await openLayers(page);
	await expect(layerSwitch(page, 'Aerial view')).toBeVisible();
	await expect(toolRow(page, 'Wind stations')).toHaveCount(0);
	await expect(toolRow(page, 'Flying sites')).toHaveCount(0);
	await closeLayers(page);
	await expect(page.locator('.notice')).toBeHidden();
	await expect(page.getByText(/^Wind: /)).toBeHidden();
});
