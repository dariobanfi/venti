/**
 * The pilot's account on the Venti server: signing in, the logbook kept in
 * step, a flight shared with a link, signing out and deleting the account.
 *
 * The server is the development mode's, behind the site as it will be when
 * one web server hands out both. It comes with a demo account and three
 * flights, and takes any registration. The demo account is only ever read
 * here: tests run side by side and again on the same server. Whatever
 * changes something on the server does so in an account of its own.
 */

import type { Page } from '@playwright/test';
import { addSampleFlight, expect, metric, test } from './helpers';

const DEMO = { handle: 'demo', password: 'demo-pilot-password' };

/** A handle nobody has used on this server: a letter, then letters and digits. */
function newHandle(): string {
	return `pilot${Math.random().toString(36).slice(2, 10)}`;
}

const field = (page: Page, name: string) => page.getByRole('textbox', { name });

/** The navigation: the account's own screen has a second link called "Settings", the way back. */
const sections = (page: Page) => page.getByRole('navigation', { name: 'Sections' });

/** Opens the account screen and signs in. */
async function signIn(page: Page, { handle, password } = DEMO): Promise<void> {
	await page.goto('/settings/account/');
	await field(page, 'Handle').fill(handle);
	await field(page, 'Password').fill(password);
	await page.getByRole('button', { name: 'Sign in' }).click();
	await expect(metric(page, 'Handle')).toHaveText(`@${handle}`);
}

/** Opens the account screen from wherever the pilot is, without loading the site anew. */
async function openAccount(page: Page): Promise<void> {
	await sections(page).getByRole('link', { name: 'Settings', exact: true }).click();
	await page.getByRole('link', { name: /^Account/ }).click();
	await expect(page.getByRole('heading', { level: 1, name: 'Account' })).toBeVisible();
}

test('a pilot signs in, gets the flights from the server, and signs out', async ({
	page,
	keep,
	problems
}) => {
	// The settings lead to the account.
	await page.goto('/');
	await sections(page).getByRole('link', { name: 'Settings', exact: true }).click();
	await expect(page).toHaveTitle('Settings · Venti');
	const account = page.getByRole('link', { name: /^Account/ });
	await expect(account).toContainText('Not signed in');
	await expect(metric(page, 'Version')).toHaveText(/^\d+\.\d+\.\d+$/);
	await keep('23-settings');

	// The form says what an account does before it asks for anything.
	await account.click();
	await expect(page).toHaveTitle('Account · Venti');
	await expect(page.getByText('A flight stays private until you share it.')).toBeVisible();
	await expect(page.getByRole('heading', { name: 'localhost:4173' })).toBeVisible();
	const submit = page.getByRole('button', { name: 'Sign in' });
	await expect(submit).toBeDisabled();
	await keep('24-account-sign-in');

	// A wrong password is refused in the core's words, and the form stays.
	await field(page, 'Handle').fill(DEMO.handle);
	await field(page, 'Password').fill('not the password');
	await submit.click();
	await expect(page.getByRole('alert')).toHaveText('The handle or the password is wrong.');
	await keep('25-account-refused');

	// The right one signs in, and the sync that follows fetches the flights.
	await field(page, 'Password').fill(DEMO.password);
	await page.keyboard.press('Enter');
	await expect(metric(page, 'Handle')).toHaveText('@demo');
	await expect(metric(page, 'Name')).toHaveText('Demo Pilot');
	await expect(metric(page, 'Server')).toHaveText('localhost:4173');
	await expect(metric(page, 'Last sync')).toHaveText('3 fetched');
	await expect(metric(page, 'Flights on the server')).toHaveText('3');
	await expect(metric(page, 'In step since')).toHaveText(/^\w{3}, \d+ \w{3}, \d\d:\d\d$/);
	await expect(page.getByText('Waiting to be sent')).toBeHidden();
	await expect(page.getByRole('button', { name: 'Sync now' })).toBeEnabled();
	await keep('26-account');

	// They are in the logbook, and the settings say whose it is.
	await sections(page).getByRole('link', { name: 'Logbook', exact: true }).click();
	await expect(metric(page.getByLabel('All flights'), 'Flights')).toHaveText('3');
	await expect(page.getByRole('main').getByRole('listitem').getByRole('link')).toHaveCount(3);
	await sections(page).getByRole('link', { name: 'Settings', exact: true }).click();
	await expect(page.getByRole('link', { name: /^Account/ })).toContainText('@demo');

	// Asked for again, a sync has nothing to do.
	await page.getByRole('link', { name: /^Account/ }).click();
	await page.getByRole('button', { name: 'Sync now' }).click();
	await expect(metric(page, 'Last sync')).toHaveText('Nothing to send or fetch');

	// Signing out asks first, and leaves the flights in the browser.
	await page.getByRole('button', { name: 'Sign out' }).click();
	const asking = page.getByRole('alertdialog', { name: 'Sign out?' });
	await expect(asking).toContainText('Your flights stay in this browser and on the server.');
	await asking.getByRole('button', { name: 'Sign out' }).click();
	await expect(page.getByRole('button', { name: 'Sign in' })).toBeVisible();
	await sections(page).getByRole('link', { name: 'Logbook', exact: true }).click();
	await expect(metric(page.getByLabel('All flights'), 'Flights')).toHaveText('3');

	expect(problems).toEqual([]);
});

test('a new account sends its flights, shares one with a link, and is deleted again', async ({
	context,
	page,
	keep,
	problems
}) => {
	const pilot = { handle: newHandle(), password: 'correct horse battery staple' };
	const more = page.getByRole('button', { name: 'More' });
	await context.grantPermissions(['clipboard-read', 'clipboard-write']);

	// A flight is in the logbook before there is an account.
	await addSampleFlight(page);
	await page.goto('/settings/account/');

	// Creating an account says what the rules are, and that a forgotten
	// password cannot be recovered.
	await page.getByRole('radio', { name: 'New account' }).check();
	await expect(page.getByText('A forgotten password cannot be recovered')).toBeVisible();
	await field(page, 'Handle').fill(pilot.handle);
	await field(page, 'Name shown to others').fill('Test Pilot');
	await field(page, 'Password').fill('too short');
	// This server says that anyone may register: no invite code is asked for.
	await expect(field(page, 'Invite code')).toBeHidden();
	await expect(page.getByText(/invite/i)).toBeHidden();
	await keep('27-account-new');

	// What the server would refuse anyway, the core refuses before it is asked.
	await page.getByRole('button', { name: 'Create account' }).click();
	await expect(page.getByRole('alert')).toContainText('at least 15 characters');

	// Created, the account is signed in and the flight is sent.
	await field(page, 'Password').fill(pilot.password);
	await page.getByRole('button', { name: 'Create account' }).click();
	await expect(metric(page, 'Handle')).toHaveText(`@${pilot.handle}`);
	await expect(metric(page, 'Name')).toHaveText('Test Pilot');
	await expect(metric(page, 'Last sync')).toHaveText('1 sent');
	await expect(metric(page, 'Flights on the server')).toHaveText('1');

	// The flight is shared from its own screen. Nothing is shared by default.
	await sections(page).getByRole('link', { name: 'Logbook', exact: true }).click();
	await page.getByRole('main').getByRole('listitem').getByRole('link').click();
	await expect(page.getByRole('heading', { name: 'Sat, 4 Jul 2026' })).toBeVisible();
	await more.click();
	await page.getByRole('menuitem', { name: 'Share with a link' }).click();
	const sheet = page.getByRole('dialog', { name: 'Share flight' });
	await expect(sheet.getByRole('radio', { name: 'Only me' })).toBeChecked();
	await expect(sheet.getByText('Nobody else can see it.')).toBeVisible();
	await expect(sheet.getByRole('heading', { name: 'Link' })).toBeHidden();

	// With a link: the link appears, and can be copied.
	await sheet.getByRole('radio', { name: 'Anyone with the link' }).check();
	const link = sheet.getByRole('link', { name: /\/f\// });
	await expect(link).toHaveText(/^http:\/\/localhost:4173\/f\/[a-z0-9]{26}$/);
	await expect(sheet.getByRole('radio', { name: 'Anyone with the link' })).toBeChecked();
	const address = (await link.textContent())!.trim();
	await sheet.getByRole('button', { name: 'Copy the link' }).click();
	await expect(sheet.getByRole('status')).toHaveText('Copied.');
	expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(address);
	await keep('28-share-flight');

	// The link leads to the page the server makes for the flight.
	const shared = await page.request.get(address);
	expect(shared.status()).toBe(200);
	expect(await shared.text()).toContain('Test Pilot');

	// Private again, the link is gone here and leads nowhere.
	await sheet.getByRole('radio', { name: 'Only me' }).check();
	await expect(sheet.getByRole('heading', { name: 'Link' })).toBeHidden();
	expect((await page.request.get(address)).status()).toBe(404);
	await sheet.getByRole('button', { name: 'Done' }).click();
	await expect(sheet).toBeHidden();

	// Deleting the flight says that the server's copy goes too, and it does.
	await more.click();
	await page.getByRole('menuitem', { name: 'Delete flight' }).click();
	const asking = page.getByRole('alertdialog', { name: 'Delete this flight?' });
	await expect(asking).toContainText(
		'with the next sync, from your Venti server and your other devices'
	);
	await asking.getByRole('button', { name: 'Delete flight' }).click();
	await expect(page.getByRole('heading', { name: 'No flights yet' })).toBeVisible();
	await openAccount(page);
	await expect(metric(page, 'Last sync')).toHaveText('1 removed on the server');
	await expect(metric(page, 'Flights on the server')).toHaveText('0');

	// Deleting the account needs the password, not just being signed in.
	await page.getByRole('button', { name: 'Delete account' }).click();
	const deleting = page.getByRole('alertdialog', { name: 'Delete your account?' });
	await expect(deleting).toContainText('deleted at once and for good');
	await expect(deleting.getByRole('button', { name: 'Delete account' })).toBeDisabled();
	await deleting.getByLabel('Password').fill('not the password');
	await deleting.getByRole('button', { name: 'Delete account' }).click();
	await expect(deleting.getByRole('alert')).toHaveText('The password is wrong.');
	await keep('29-account-delete');
	await deleting.getByLabel('Password').fill(pilot.password);
	await deleting.getByRole('button', { name: 'Delete account' }).click();
	await expect(deleting).toBeHidden();
	await expect(page.getByRole('button', { name: 'Sign in' })).toBeVisible();

	// It is gone: nobody signs in to it any more.
	await field(page, 'Handle').fill(pilot.handle);
	await field(page, 'Password').fill(pilot.password);
	await page.getByRole('button', { name: 'Sign in' }).click();
	await expect(page.getByRole('alert')).toHaveText('The handle or the password is wrong.');

	// Nothing went wrong but what was meant to: the passwords the server
	// refused, which the browser notes as failed requests.
	expect(problems.filter((problem) => !/status of 40[13]/.test(problem))).toEqual([]);
});

test('the logbook follows the server when the site is opened and comes back to the front', async ({
	page
}) => {
	await signIn(page);
	await expect(metric(page, 'Last sync')).toHaveText('3 fetched');
	const asksForChanges = () =>
		page.waitForRequest((request) => request.url().includes('/v1/flights/changes'));

	// Opened again, the site asks the server what changed, without being told to.
	let asked = asksForChanges();
	await page.reload();
	expect((await asked).headers().authorization).toMatch(/^Bearer venti_/);
	await expect(metric(page, 'Flights on the server')).toHaveText('3');

	// So it does when the pilot comes back to the tab after a while.
	asked = asksForChanges();
	await sections(page).getByRole('link', { name: 'Logbook', exact: true }).click();
	await expect(metric(page.getByLabel('All flights'), 'Flights')).toHaveText('3');
	await page.evaluate(() => {
		// More than the minute that lies between two syncs nobody asked for.
		const later = Date.now() + 61_000;
		Date.now = () => later;
		document.dispatchEvent(new Event('visibilitychange'));
	});
	await asked;
});

test('a sync that fails says why, and a session that ended asks to sign in again', async ({
	context,
	page,
	keep
}) => {
	await signIn(page);
	await expect(metric(page, 'Last sync')).toHaveText('3 fetched');

	// Out of reach.
	await context.route('**/v1/flights/changes**', (route) => route.abort());
	await page.getByRole('button', { name: 'Sync now' }).click();
	await expect(page.getByRole('alert')).toHaveText('The server could not be reached.');
	await expect(page.getByRole('button', { name: 'Sync now' })).toBeEnabled();

	// The server asks to wait.
	await context.unroute('**/v1/flights/changes**');
	await context.route('**/v1/flights/changes**', (route) =>
		route.fulfill({
			status: 429,
			headers: { 'retry-after': '120' },
			json: { error: { code: 'rate_limited', message: 'slow down' } }
		})
	);
	await page.getByRole('button', { name: 'Sync now' }).click();
	await expect(page.getByRole('alert')).toHaveText(
		'The server asks to wait 2 minutes. What was done so far is kept.'
	);

	// The session has ended on the server: the core says so and the screen
	// says what to do. Nothing more is asked until the pilot has signed in again.
	await context.unroute('**/v1/flights/changes**');
	await context.route('**/v1/flights/changes**', (route) =>
		route.fulfill({
			status: 401,
			json: { error: { code: 'unauthorized', message: 'no session' } }
		})
	);
	await page.getByRole('button', { name: 'Sync now' }).click();
	await expect(page.getByRole('alert')).toHaveText(
		'This browser was signed out. Sign out here and sign in again.'
	);
	await expect(page.getByRole('button', { name: 'Sync now' })).toBeDisabled();
	await keep('30-account-signed-out');
	await context.unroute('**/v1/flights/changes**');

	// The flights fetched before are still in the logbook.
	await sections(page).getByRole('link', { name: 'Settings', exact: true }).click();
	await expect(page.getByRole('link', { name: /^Account/ })).toContainText('Signed out');
});

test('a flight cannot be shared before the pilot is signed in, and the sheet says where to', async ({
	page,
	keep
}) => {
	await addSampleFlight(page);
	await page.getByRole('button', { name: 'More' }).click();
	await page.getByRole('menuitem', { name: 'Share with a link' }).click();

	const sheet = page.getByRole('dialog', { name: 'Share flight' });
	await expect(sheet).toContainText('To share a flight with a link, sign in');
	await keep('31-share-signed-out');
	await sheet.getByRole('link', { name: 'Account' }).click();
	await expect(page).toHaveTitle('Account · Venti');
	await expect(page.getByRole('button', { name: 'Sign in' })).toBeVisible();
});

test('the form for a new account follows who may register on the server', async ({
	context,
	page,
	keep
}) => {
	// The server says who may create an account in its health answer. The
	// development mode's says "open"; here it says each of the others in turn.
	let registration: string | undefined = 'invite';
	await context.route('**/v1/health', async (route) => {
		const response = await route.fetch();
		const health = await response.json();
		if (registration === undefined) delete health.registration;
		else health.registration = registration;
		await route.fulfill({ response, json: health });
	});

	// By invite: the field for the code, and why it is there.
	await page.goto('/settings/account/');
	await page.getByRole('radio', { name: 'New account' }).check();
	await expect(field(page, 'Invite code')).toBeVisible();
	await expect(
		page.getByText('This server takes new accounts by invite. Whoever runs it hands the codes out.')
	).toBeVisible();
	await keep('52-account-new-by-invite');

	// Closed: no form to fill in, and it says so. Signing in is as before.
	registration = 'closed';
	await page.reload();
	await page.getByRole('radio', { name: 'New account' }).check();
	await expect(
		page.getByText(
			'This server takes no new accounts. An account you already have signs in as before.'
		)
	).toBeVisible();
	await expect(field(page, 'Handle')).toBeHidden();
	await expect(field(page, 'Invite code')).toBeHidden();
	await expect(page.getByRole('button', { name: 'Create account' })).toBeHidden();
	await keep('53-account-new-closed');
	await page.getByRole('radio', { name: 'Sign in' }).check();
	await expect(field(page, 'Handle')).toBeVisible();
	await expect(page.getByRole('button', { name: 'Sign in' })).toBeVisible();

	// A server older than the question says nothing, and may ask for a
	// code: the field is offered, as it always was.
	registration = undefined;
	await page.reload();
	await page.getByRole('radio', { name: 'New account' }).check();
	await expect(field(page, 'Invite code')).toBeVisible();
	await expect(
		page.getByText('The invite code is for a server that takes accounts by invite.')
	).toBeVisible();
});

test('a site without a server has no account to sign in to, and says so', async ({
	context,
	page,
	keep
}) => {
	// What a web server answers that only hands out the app's files.
	await context.route('**/v1/**', (route) => route.fulfill({ status: 404, body: 'Not found' }));
	await page.goto('/settings/');
	await expect(page.getByText('This site has no Venti server behind it.')).toBeVisible();

	await page.getByRole('link', { name: /^Account/ }).click();
	await expect(
		page.getByText('This site has no Venti server behind it, so there is no account')
	).toBeVisible();
	await expect(page.getByRole('button', { name: 'Sign in' })).toBeHidden();
	await keep('32-account-no-server');
});
