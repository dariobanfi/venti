/**
 * Pilots among each other: following a pilot, the feed of the pilots one
 * follows, cheers and comments under a public flight, what the pilot was
 * told, and the row that lets this browser be told of an alarm.
 *
 * In the development mode the demo pilot follows Mara Muster, Jonas
 * Beispiel and Lena Probe, the newest public flights of the three have a
 * few cheers and comments, and the demo account was told of a follow, a
 * cheer and a comment. The demo account is only read: the same test runs
 * at several widths on one server, so the test that follows, cheers and
 * writes makes an account of its own.
 */

import type { Page } from '@playwright/test';
import { expect, metric, rowsOf, test } from './helpers';

const DEMO = { handle: 'demo', password: 'demo-pilot-password' };

const field = (page: Page, name: string) => page.getByRole('textbox', { name, exact: true });

async function signIn(page: Page): Promise<void> {
	await page.goto('/settings/account/');
	await field(page, 'Handle').fill(DEMO.handle);
	await field(page, 'Password').fill(DEMO.password);
	await page.getByRole('button', { name: 'Sign in', exact: true }).click();
	await expect(metric(page, 'Handle')).toHaveText(`@${DEMO.handle}`);
}

async function signUp(page: Page): Promise<string> {
	const handle = `pilot${Math.random().toString(36).slice(2, 10)}`;
	await page.goto('/settings/account/');
	await page.getByRole('radio', { name: 'New account' }).check();
	await field(page, 'Handle').fill(handle);
	await field(page, 'Name shown to others').fill('Test Pilot');
	await field(page, 'Password').fill('correct horse battery staple');
	await page.getByRole('button', { name: 'Create account' }).click();
	await expect(metric(page, 'Handle')).toHaveText(`@${handle}`);
	return handle;
}

const feed = (page: Page) => rowsOf(page, 'Following');

test('the demo pilot sees whom they follow, their feed and what they were told', async ({
	page,
	keep,
	problems
}) => {
	await signIn(page);

	// The home page has the newest flights of the pilots followed.
	await page.goto('/');
	const home = page.getByRole('region', { name: 'Following' });
	// Rows of a list at a phone's width, of a table at a desktop's.
	await expect(
		home
			.getByRole('table', { name: 'Following' })
			.locator('tbody > tr')
			.or(home.getByRole('list', { name: 'Following' }).getByRole('listitem'))
			.first()
	).toBeVisible();
	await expect(home).toContainText(/cheered/);
	await home.scrollIntoViewIfNeeded();
	await keep('270-home-following');

	// The whole feed, beside "Everyone".
	await home.getByRole('link', { name: 'All', exact: true }).click();
	await expect(page).toHaveURL(/\/flights\/\?feed=following$/);
	await expect(page.getByRole('radio', { name: 'Following' })).toBeChecked();
	await expect(feed(page).first()).toBeVisible();
	await expect(feed(page).filter({ hasText: 'Mara Muster' }).first()).toContainText(/cheered/);
	await keep('271-feed');
	await page.emulateMedia({ colorScheme: 'dark' });
	await keep('272-feed-dark');
	// Back to the project's own appearance, which may be dark.
	await page.emulateMedia({ colorScheme: test.info().project.use.colorScheme ?? 'light' });

	// "Everyone" is the list as it was.
	await page.getByRole('radio', { name: 'Everyone', exact: true }).check();
	await expect(page).toHaveURL(/\/flights\/$/);
	await expect(rowsOf(page, 'Everyone’s flights').first()).toBeVisible();

	// Community has the same: the plans of the demo pilot's group, the
	// feed as cards with the track's shape, finding a pilot, and the
	// newest of what the pilot was told. The section's name ends in a
	// count of unread messages, which the sample pilots' messages and
	// other tests running beside this one change.
	await page
		.getByRole('navigation', { name: 'Sections' })
		.getByRole('link', { name: /^Community/ })
		.click();
	await expect(page.getByRole('heading', { level: 1, name: 'Community' })).toBeVisible();
	await expect(page.getByRole('region', { name: 'Plans', exact: true })).toBeVisible();
	const cards = rowsOf(page.getByRole('region', { name: 'Following' }), 'Following');
	await expect(cards.first()).toBeVisible();
	await expect(cards.first().locator('svg polyline')).toHaveCount(1);
	await expect(cards.filter({ hasText: 'Mara Muster' }).first()).toContainText(/\d+:\d\dh/);
	// Mara has a picture, beside her name on her cards.
	await expect(cards.filter({ hasText: 'Mara Muster' }).first().locator('img')).toHaveAttribute(
		'src',
		/^\/v1\/pictures\//
	);
	await expect(page.getByRole('searchbox', { name: 'Find a pilot' })).toBeVisible();
	const newest = page.getByRole('region', { name: 'Notifications' });
	await expect(newest.getByRole('list', { name: 'Notifications' })).toContainText(
		/cheered your flight|follows you|commented/
	);
	await keep('270b-community');
	await page.emulateMedia({ colorScheme: 'dark' });
	await keep('270c-community-dark');
	await page.emulateMedia({ colorScheme: test.info().project.use.colorScheme ?? 'light' });
	// A card opens the flight.
	await cards.first().locator('a.open').click();
	await expect(page).toHaveURL(/\/\/[^/]+\/flights\/[0-9a-z]{26}\//);

	// A pilot's page says who follows them, and that the demo pilot does.
	await page.goto('/pilots/mara/');
	await expect(page.getByRole('heading', { name: 'Mara Muster', level: 1 })).toBeVisible();
	await expect(page.getByRole('button', { name: 'Following' })).toHaveAttribute(
		'aria-pressed',
		'true'
	);
	await expect(page.getByText(/^\d+ followers?$/)).toBeVisible();
	await expect(page.getByText(/^Follows \d+ pilots?$/)).toBeVisible();
	await keep('273-pilot-follow');

	// Nobody follows themselves: one's own page has the counts alone.
	await page.goto('/pilots/demo/');
	await expect(page.getByText(/^\d+ followers?$/)).toBeVisible();
	await expect(page.getByRole('button', { name: /^Follow/ })).toHaveCount(0);

	// What the pilot was told, under the account.
	await page.goto('/settings/account/');
	await page.locator('.settings-page').getByRole('link', { name: 'Notifications' }).click();
	await expect(page.getByRole('heading', { name: 'Notifications', level: 1 })).toBeVisible();
	const told = page.getByRole('list', { name: 'Notifications' });
	await expect(told.getByRole('listitem')).not.toHaveCount(0);
	await expect(told).toContainText(/Lena Probe \(@lena\) follows you/);
	await expect(told).toContainText(/cheered your flight/);
	await keep('274-notifications');

	// The alarms' page has the row that lets this browser be told. A test
	// browser grants no push, so the row is only looked at.
	await page.goto('/settings/account/alerts/');
	await expect(page.getByRole('switch', { name: 'Notify this browser' })).toBeVisible();
	await page.getByRole('switch', { name: 'Notify this browser' }).scrollIntoViewIfNeeded();
	await keep('275-alerts-notify-this-browser');

	expect(problems).toEqual([]);
});

test('a pilot follows another, cheers a flight and writes under it', async ({
	page,
	keep,
	wide,
	problems
}) => {
	await signUp(page);

	// Signed in, another pilot's page has the button.
	await page.goto('/pilots/mara/');
	const follow = page.getByRole('button', { name: 'Follow', exact: true });
	await expect(follow).toHaveAttribute('aria-pressed', 'false');
	await follow.click();
	await expect(page.getByRole('button', { name: 'Following' })).toHaveAttribute(
		'aria-pressed',
		'true'
	);

	// The feed now has Mara Muster's flights.
	await page.goto('/flights/');
	await page.getByRole('radio', { name: 'Following' }).check();
	const first = feed(page).filter({ hasText: 'Mara Muster' }).first();
	await expect(first).toBeVisible();

	// Her newest flight: cheers and comments beside the flight on a wide
	// screen, in a sheet on a phone.
	await first.locator('a.flight').click();
	await expect(page).toHaveURL(/\/\/[^/]+\/flights\/[0-9a-z]{26}\//);
	if (!wide) await page.getByRole('button', { name: /cheered|comment/i }).click();
	const said = page.getByRole('region', { name: 'Comments' });
	await expect(said).toContainText(/cheered/);
	await expect(said).toContainText('A made-up comment of the development mode.');

	await said.getByRole('button', { name: 'Cheer', exact: true }).click();
	await expect(said.getByRole('button', { name: 'Cheered' })).toHaveAttribute(
		'aria-pressed',
		'true'
	);
	await expect(said).toContainText(/^.*You and \d+ others? cheered/s);

	// A comment is plain text: markup in it is shown as it was typed.
	const words = `Well flown <img src=x> ${Math.random().toString(36).slice(2, 8)}`;
	await said.getByRole('textbox', { name: 'Write a comment' }).fill(words);
	await expect(said).toContainText(/\d+ of \d+/);
	await said.getByRole('button', { name: 'Send' }).click();
	const mine = said.getByRole('listitem').filter({ hasText: words });
	await expect(mine).toBeVisible();
	await expect(mine).toContainText('Test Pilot');
	await expect(said.locator('img')).toHaveCount(0);
	await keep('276-flight-cheers-comments');

	// Another pilot's comment can be reported; one's own removed.
	await expect(
		said
			.getByRole('listitem')
			.filter({ hasText: 'A made-up comment of the development mode.' })
			.getByRole('button', { name: 'Report' })
	).toBeVisible();
	// The core asks first.
	await mine.getByRole('button', { name: 'Remove' }).click();
	const asked = page.getByRole('alertdialog', { name: 'Remove this comment?' });
	await asked.getByRole('button', { name: 'Remove' }).click();
	await expect(mine).toHaveCount(0);

	// The cheer is taken back, so that the made-up flight stays as it was.
	await said.getByRole('button', { name: 'Cheered' }).click();
	await expect(said.getByRole('button', { name: 'Cheer', exact: true })).toBeVisible();

	expect(problems).toEqual([]);
});

test('what is new shows on the pilot’s picture until the notifications are seen', async ({
	page,
	keep,
	wide,
	problems
}) => {
	const handle = await signUp(page);
	// The server's count is played here: one new notification, none once
	// seen. The server's own tests hold what it counts.
	const seen: unknown[] = [];
	let news = 1;
	await page.route('**/v1/account/unread', async (route) => {
		if (route.request().method() === 'PUT') {
			seen.push(route.request().postDataJSON());
			news = 0;
		}
		await route.fulfill({
			status: 200,
			contentType: 'application/json',
			body: JSON.stringify({ notifications: news, alarms: 0, total: news })
		});
	});
	const marked = page
		.getByRole('navigation', { name: 'Sections' })
		.getByRole('link', { name: /1 new/ });

	// Read when the page opens: on the picture in the wide bar, on Settings
	// in a phone's, which has no picture.
	await page.goto('/');
	await expect(marked).toBeVisible();
	await expect(marked.locator('.bubble')).toHaveText('1');
	await expect(marked).toHaveAccessibleName(wide ? `${handle} 1 new` : 'Settings 1 new');
	await keep('277-unread-marker');

	// The notifications seen, up to the cursor of what was shown, and the
	// alarms' mark left where it is (the core leaves its cursor out): the
	// bubble goes.
	await page.goto('/settings/account/notifications/');
	await expect(page.getByRole('heading', { name: 'Notifications', level: 1 })).toBeVisible();
	await expect.poll(() => seen.length).toBe(1);
	expect(seen[0]).toHaveProperty('notifications_cursor');
	expect(seen[0]).not.toHaveProperty('alarms_cursor');
	await expect(marked).toHaveCount(0);
	await keep('278-unread-seen');

	expect(problems).toEqual([]);
});
