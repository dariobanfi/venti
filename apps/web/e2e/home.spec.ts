/**
 * The home page, from the Venti server in its development mode: what Venti
 * stands for, the season's flights that stand out, the weather and the
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

	// What Venti stands for, in four lines, with the way to the about page.
	const standsFor = page.getByRole('region', { name: 'What Venti stands for' });
	await expect(standsFor.getByRole('listitem')).toHaveCount(4);
	await expect(standsFor.getByRole('link', { name: 'What Venti stands for' })).toHaveAttribute(
		'href',
		/\/about\/?$/
	);

	// The season's flights that stand out: its longest, and the newest, since
	// none of the made-up flights is of the last days or a triangle.
	const achievements = page.getByRole('region', { name: 'Achievements' });
	const cards = achievements.getByRole('listitem');
	await expect(cards).toHaveCount(2);
	await expect(cards.first()).toContainText('Longest flight, 2026');
	await expect(cards.first()).toContainText('Lena Probe');
	await expect(cards.first().getByRole('link', { name: /km$/ })).toHaveAttribute(
		'href',
		/\/logbook\/everyone\/flight\/?\?id=/
	);
	await expect(cards.nth(1)).toContainText('Newest flight');

	// The weather: the best take-off around the region's middle, as Today's
	// core judges it, its wind and the next hours, and when the model ran.
	const weather = panel(page, 'Weather');
	await expect(weather.getByText(/, around the Tegernsee$/)).toBeVisible();
	await expect(weather.locator('.site')).toContainText(/(Flyable|Marginal|Not flyable)$/);
	await expect(weather.getByRole('list', { name: 'Hour by hour' })).toBeVisible();
	await expect(weather.getByText(/ICON-(D2|EU) of /)).toBeVisible();
	await expect(weather.getByRole('link', { name: 'Today' })).toBeVisible();
	await expect(weather.getByRole('button', { name: 'Use my position' })).toBeVisible();

	// Venti's news: the first post, still a draft.
	const news = panel(page, 'News');
	await expect(news.getByRole('link', { name: 'Welcome to Venti' })).toHaveAttribute(
		'href',
		/\/news\/2026-11-22-welcome-to-venti\/?$/
	);
	await expect(news.getByText('Draft', { exact: true })).toBeVisible();

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

test('refused a position, the weather offers a place to choose', async ({ page, keep }) => {
	await page.goto('/');
	const weather = panel(page, 'Weather');
	await expect(weather.getByRole('list', { name: 'Hour by hour' })).toBeVisible();

	// The test's browser gives no position: one line, and a choice of place.
	await weather.getByRole('button', { name: 'Use my position' }).click();
	await expect(weather.getByText('Your browser does not give Venti your position.')).toBeVisible();
	await expect(weather.getByRole('link', { name: 'On the map' })).toHaveAttribute(
		'href',
		/\/map\/?\?lat=/
	);
	const choice = weather.getByRole('combobox', { name: 'Choose a take-off' });
	await weather.scrollIntoViewIfNeeded();
	await keep('202-home-weather-refused');

	// A take-off chosen: the weather is for it, and stays so on the next visit.
	const name = (await choice.locator('option').nth(2).textContent())?.trim() ?? '';
	await choice.selectOption({ label: name });
	await expect(weather.getByText(/, the take-off you chose$/)).toBeVisible();
	await expect(weather.getByRole('link', { name, exact: true })).toBeVisible();
	await page.reload();
	await expect(panel(page, 'Weather').getByRole('link', { name, exact: true })).toBeVisible();
});

test('given a position, the weather is for the pilot’s place', async ({ page, context, keep }) => {
	await context.grantPermissions(['geolocation']);
	await context.setGeolocation({ latitude: 47.6642, longitude: 11.7967 });
	await page.goto('/');
	const weather = panel(page, 'Weather');
	await expect(weather.getByText(/, near you$/)).toBeVisible();
	await expect(weather.getByRole('list', { name: 'Hour by hour' })).toBeVisible();
	await expect(weather.getByRole('button', { name: 'Use my position' })).toHaveCount(0);
	await weather.scrollIntoViewIfNeeded();
	await keep('203-home-weather-near-you');
});

test('an address of the map from before goes to the map', async ({ page }) => {
	await page.goto('/?lat=47.70&lon=11.76');
	await expect(page).toHaveURL(/\/map\/\?lat=47\.70&lon=11\.76$/);
	await expect(page).toHaveTitle('Map · Venti');
});
