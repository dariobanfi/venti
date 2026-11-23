/**
 * The home page, from the Venti server in its development mode: Today
 * first (the region's take-offs and what the forecast says of each; its own
 * tests are `today.spec.ts`), what Venti stands for, the season's flights
 * that stand out, its made-up pilots in the air and the Open Glider
 * Network's made-up aircraft over the Tegernsee, the public flights and
 * the ranking of its six made-up pilots, the made-up place from pilots
 * near Garmisch and the take-offs without a site, the recorded thermal
 * map and radar. Nobody shared a flight of the browser's today, so the
 * flights are the newest.
 */

import type { Page } from '@playwright/test';
import { expect, metric, test } from './helpers';

const panel = (page: Page, name: string | RegExp, exact = false) =>
	page.getByRole('region', { name, exact });

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
		/^\/flights\/[0-9a-z]{26}\/$/
	);
	await expect(cards.nth(1)).toContainText('Newest flight');

	// Today first: the take-offs around the region's middle as the core
	// judges them, the best one's hours, and when the model ran.
	const today = panel(page, 'Today', true);
	await expect(today.getByText(/, around the Tegernsee$/)).toBeVisible();
	await expect(today.getByText(/^(Flyable|Marginal|Not flyable)$/).first()).toBeVisible({
		timeout: 30_000
	});
	await expect(today.getByRole('list', { name: 'Hour by hour' })).toBeVisible();
	await expect(today.getByText(/ICON-(D2|EU) of /)).toBeVisible();
	await expect(today.getByRole('button', { name: 'Use my position' })).toBeVisible();
	expect((await today.boundingBox())!.y).toBeLessThan((await standsFor.boundingBox())!.y);
	await expect(panel(page, 'Weather')).toHaveCount(0);
	await expect(page.locator('#home-today')).toHaveCount(0);

	// Venti's news: the first post, still a draft.
	const news = panel(page, 'News');
	await expect(news.getByRole('link', { name: 'Welcome to Venti' })).toHaveAttribute(
		'href',
		/\/news\/2026-11-22-welcome-to-venti\/?$/
	);
	await expect(news.getByText('Draft', { exact: true })).toBeVisible();

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
			/^\/flights\/[0-9a-z]{26}\/$/
		);
	} else {
		await expect(flights.getByRole('table')).toHaveCount(0);
		const rows = flights.getByRole('list', { name: 'Flights' }).getByRole('listitem');
		await expect(rows).toHaveCount(10);
		await expect(rows.first().locator('a.flight')).toHaveAttribute(
			'href',
			/^\/flights\/[0-9a-z]{26}\/$/
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

test('today’s old address leads home, and the bar has no section of its own for it', async ({
	page,
	keep
}) => {
	await page.goto('/today/');
	await expect(page).toHaveURL(/\/\/[^/]+\/$/);
	const sections = page.getByRole('navigation', { name: 'Sections' });
	await expect(sections.getByRole('link', { name: 'Today' })).toHaveCount(0);
	// Six sections at every width: the ranking has Today's place in the phone's bar.
	await expect(sections.getByRole('link', { name: 'Ranking' })).toBeVisible();
	await expect(page.getByRole('region', { name: 'Today', exact: true })).toBeVisible();
	await keep('200a-home-today-first');
});

test('refused a position, Today says why and stays with the region', async ({ page }) => {
	await page.goto('/');
	const today = panel(page, 'Today', true);
	await expect(today.getByRole('list', { name: 'Hour by hour' })).toBeVisible({ timeout: 30_000 });
	// The test's browser gives no position: one line, and the region's take-offs stay.
	await today.getByRole('button', { name: 'Use my position' }).click();
	await expect(
		today.getByText(/allowed to use your position|does not give your position/)
	).toBeVisible();
	await expect(today.getByText(/, around the Tegernsee$/)).toBeVisible();
});

test('given a position, Today is for the pilot’s place', async ({ page, context, keep }) => {
	await context.grantPermissions(['geolocation']);
	await context.setGeolocation({ latitude: 47.6642, longitude: 11.7967 });
	await page.goto('/');
	const today = panel(page, 'Today', true);
	await expect(today.getByText(/, near you$/)).toBeVisible();
	await expect(today.getByRole('list', { name: 'Hour by hour' })).toBeVisible({ timeout: 30_000 });
	await expect(today.getByRole('button', { name: 'Use my position' })).toHaveCount(0);
	await keep('203a-home-today-near-you');
});

test('an address of the map from before goes to the map', async ({ page }) => {
	await page.goto('/?lat=47.70&lon=11.76');
	await expect(page).toHaveURL(/\/map\/\?lat=47\.70&lon=11\.76$/);
	await expect(page).toHaveTitle('Map · Venti');
});
