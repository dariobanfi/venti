/**
 * The home page, from the Venti server in its development mode: the
 * region's take-offs, its made-up pilots in the air and the Open Glider
 * Network's made-up aircraft over the Tegernsee, the public flights and
 * the ranking of its six made-up pilots, the made-up place from pilots
 * near Garmisch and the take-offs without a site, the recorded thermal
 * map and radar. Nobody shared a flight of the browser's today, so the
 * flights are the newest.
 */

import type { Page } from '@playwright/test';
import { expect, metric, test } from './helpers';

const panel = (page: Page, name: string | RegExp) => page.getByRole('region', { name });

test('the home page shows what is going on, panel by panel', async ({
	page,
	keep,
	wide,
	problems
}) => {
	await page.goto('/');
	await expect(page).toHaveTitle('Venti');
	await expect(page.getByRole('heading', { level: 1, name: 'What is going on' })).toBeVisible();
	const sections = page.getByRole('navigation', { name: 'Sections' });
	await expect(sections.getByRole('link', { name: 'Home', exact: true })).toHaveAttribute(
		'aria-current',
		'page'
	);

	// Today's take-offs, as the core judges them, with the way to Today.
	const today = panel(page, /^Take-offs/);
	await expect(today.getByRole('table', { name: 'Take-offs' }).getByRole('row')).toHaveCount(5);
	// Each row has the core's verdict as its chip; no line under the table repeats it.
	await expect(
		today.getByRole('cell', { name: /^(Flyable|Marginal|Not flyable)$/ }).first()
	).toBeVisible();
	await expect(today.getByRole('link', { name: 'Today' })).toBeVisible();

	// Who is in the air: the core's count of Venti's pilots and the others.
	const live = panel(page, 'In the air now');
	await expect(live.getByText(/pilots? of Venti, \d+ others? in the air$/)).toBeVisible();
	await expect(live.getByRole('link', { name: 'On the map' })).toHaveAttribute(
		'href',
		/\/map\?show=pilots$/
	);

	// The newest flights, each leading to the flight: a table on a wide
	// screen, the rows everyone's flights have on a phone.
	const flights = panel(page, 'Flown lately');
	if (wide) {
		const rows = flights.getByRole('table', { name: 'Flights' }).getByRole('row');
		await expect(rows).toHaveCount(11);
		await expect(rows.nth(1).getByRole('link', { name: /km$/ })).toHaveAttribute(
			'href',
			/\/logbook\/everyone\/flight\/?\?id=/
		);
	} else {
		await expect(flights.getByRole('table')).toHaveCount(0);
		const rows = flights.getByRole('list', { name: 'Flights' }).getByRole('listitem');
		await expect(rows).toHaveCount(10);
		await expect(rows.first().locator('a.flight')).toHaveAttribute(
			'href',
			/\/logbook\/everyone\/flight\/?\?id=/
		);
	}

	// The top of the season.
	const ranking = panel(page, /^Ranking/);
	await expect(ranking.getByRole('table', { name: 'Ranking' }).getByRole('row')).toHaveCount(6);
	await expect(ranking.getByRole('row').first()).toContainText('Lena Probe');

	// What pilots added, and where they take off without a site.
	const community = panel(page, 'From the community');
	await expect(community.getByRole('link', { name: 'Beispieljoch' })).toBeVisible();
	await expect(community.getByText(/No site names this place\.$/).first()).toBeVisible();

	// The weather at a glance: the thermal map's day and the radar's newest frame.
	const weather = panel(page, 'Weather');
	await expect(weather.getByText(/ICON-(D2|EU) of /)).toBeVisible();
	await expect(weather.getByText(/^Rain radar, /)).toBeVisible();

	// Signed out, no alarms.
	await expect(panel(page, 'Your alarms')).toHaveCount(0);
	await keep('200-home');

	expect(problems).toEqual([]);
});

test('signed in, the home page has the pilot’s alarms', async ({ page, keep, wide }) => {
	await page.goto('/settings/account/');
	await page.getByRole('textbox', { name: 'Handle', exact: true }).fill('demo');
	await page.getByRole('textbox', { name: 'Password', exact: true }).fill('demo-pilot-password');
	await page.getByRole('button', { name: 'Sign in', exact: true }).click();
	await expect(metric(page, 'Handle')).toHaveText('@demo');

	await page
		.getByRole('navigation', { name: 'Sections' })
		.getByRole('link', { name: 'Home' })
		.click();
	await expect(panel(page, 'Your alarms')).toBeVisible();
	await expect(panel(page, 'Ranking 2026')).toBeVisible();
	await keep('201-home-signed-in');

	// What the pilots followed flew, with the core's lines of cheers and
	// comments; the link says where it leads, not the heading again.
	const following = panel(page, 'Following');
	await expect(following.getByRole('link', { name: 'All', exact: true })).toHaveAttribute(
		'href',
		/feed=following/
	);
	await expect(following).toContainText(/cheered/);
	if (!wide) {
		await expect(following.getByRole('table')).toHaveCount(0);
		await expect(
			following.getByRole('list', { name: 'Following' }).getByRole('listitem').first()
		).toBeVisible();
	}
	await following.scrollIntoViewIfNeeded();
	await keep('222-home-following');
});

test('an address of the map from before goes to the map', async ({ page }) => {
	await page.goto('/?lat=47.70&lon=11.76');
	await expect(page).toHaveURL(/\/map\/\?lat=47\.70&lon=11\.76$/);
	await expect(page).toHaveTitle('Map · Venti');
});
