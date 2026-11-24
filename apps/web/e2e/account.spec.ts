/**
 * The pilot's account on the Venti server: signing in, the pilot's flights
 * on it, a flight shared with a link, signing out and deleting the account.
 *
 * The server is the development mode's, behind the site as it will be when
 * one web server hands out both. It comes with a demo account and three
 * flights, and takes any registration. The demo account is only ever read
 * here: tests run side by side and again on the same server. Whatever
 * changes something on the server does so in an account of its own.
 */

import type { Page } from '@playwright/test';
import { asUpload, expect, metric, SAMPLE_FLIGHT, signUp, test } from './helpers';
import { readFileSync } from 'node:fs';
import { deflateSync } from 'node:zlib';

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
	await page.getByRole('button', { name: 'Sign in', exact: true }).click();
	await expect(metric(page, 'Handle')).toHaveText(`@${handle}`);
}

/** Opens the account screen from wherever the pilot is, without loading the site anew. */
async function openAccount(page: Page): Promise<void> {
	await sections(page)
		.getByRole('link', { name: /^Settings/ })
		.click();
	await page
		.locator('.settings-page')
		.getByRole('link', { name: /^Account/ })
		.click();
	await expect(page.getByRole('heading', { level: 1, name: 'Account' })).toBeVisible();
}

test('a pilot signs in, gets the flights from the server, and signs out', async ({
	page,
	keep,
	problems
}) => {
	// The settings lead to the account.
	await page.goto('/map/');
	await sections(page)
		.getByRole('link', { name: /^Settings/ })
		.click();
	await expect(page).toHaveTitle('Settings · Venti');
	const account = page.locator('.settings-page').getByRole('link', { name: /^Account/ });
	await expect(account).toContainText('Not signed in');
	await keep('23-settings');

	// The form says what an account does before it asks for anything.
	await account.click();
	await expect(page).toHaveTitle('Account · Venti');
	await expect(page.getByText('you can share one with a link.')).toBeVisible();
	await expect(page.getByRole('heading', { name: 'localhost:4173' })).toBeVisible();
	const submit = page.getByRole('button', { name: 'Sign in', exact: true });
	await expect(submit).toBeDisabled();
	await keep('24-account-sign-in');

	// A wrong password is refused in the core's words, and the form stays.
	await field(page, 'Handle').fill(DEMO.handle);
	await field(page, 'Password').fill('not the password');
	await submit.click();
	await expect(page.getByRole('alert')).toHaveText('The handle or the password is wrong.');
	await keep('25-account-refused');

	// The right one signs in. The website keeps no flights of its own: the
	// account's are on the server, behind "My flights".
	await field(page, 'Password').fill(DEMO.password);
	await page.keyboard.press('Enter');
	await expect(metric(page, 'Handle')).toHaveText('@demo');
	await expect(metric(page, 'Name')).toHaveText('Demo Pilot');
	// There is one Venti: the account shows no server address.
	await expect(page.getByText('Address', { exact: true })).toBeHidden();
	await expect(page.getByText('Last sync')).toBeHidden();
	await expect(page.getByRole('button', { name: 'Sync now' })).toBeHidden();
	await keep('26-account');

	// The account's flights, by day.
	await page.getByRole('link', { name: 'My flights' }).click();
	await expect(page.getByRole('heading', { level: 1, name: 'My flights' })).toBeVisible();
	await expect(page.getByRole('main').locator('section.day li')).toHaveCount(3);
	await sections(page)
		.getByRole('link', { name: /^Settings/ })
		.click();
	await expect(
		page.locator('.settings-page').getByRole('link', { name: /^Account/ })
	).toContainText('@demo');

	await page
		.locator('.settings-page')
		.getByRole('link', { name: /^Account/ })
		.click();

	// Signing out asks first; the flights stay on the account, and the
	// browser shows them no more.
	await page.getByRole('button', { name: 'Sign out' }).click();
	const asking = page.getByRole('alertdialog', { name: 'Sign out?' });
	await expect(asking).toContainText('Your flights stay on your Venti account.');
	await asking.getByRole('button', { name: 'Sign out' }).click();
	await expect(page.getByRole('button', { name: 'Sign in', exact: true })).toBeVisible();
	await page.goto('/settings/account/flights/');
	await expect(page.getByRole('heading', { name: 'Sign in to see your flights' })).toBeVisible();

	expect(problems).toEqual([]);
});

test('a new account brings a flight in, shares it with a link, and is deleted again', async ({
	context,
	page,
	keep,
	problems
}) => {
	const pilot = { handle: newHandle(), password: 'correct horse battery staple' };
	const more = page.getByRole('button', { name: 'More' });
	await context.grantPermissions(['clipboard-read', 'clipboard-write']);

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

	// Created, the account is signed in, and a flight brought in goes to it.
	await field(page, 'Password').fill(pilot.password);
	await page.getByRole('button', { name: 'Create account' }).click();
	await expect(metric(page, 'Handle')).toHaveText(`@${pilot.handle}`);
	await expect(metric(page, 'Name')).toHaveText('Test Pilot');
	// New flights are public until the pilot says otherwise; this one is kept
	// to the pilot, as everyone's flights are counted by other tests.
	const open = page.getByRole('switch', { name: 'New flights are public' });
	await expect(open).toBeChecked();
	await open.uncheck();
	await expect(open).not.toBeChecked();
	await expect(open).toBeEnabled();
	await page.getByRole('link', { name: 'My flights' }).click();
	// The account's screen has a file input of its own, for the picture.
	await expect(page).toHaveURL(/\/settings\/account\/flights\/$/);
	await page
		.locator('input[type=file]')
		.setInputFiles(asUpload('sample.igc', readFileSync(SAMPLE_FLIGHT, 'utf8')));

	// It opens at once. Who sees a new flight is the account's setting (owner
	// decision 21), switched off above: the sheet says so, and the pilot
	// changes it here.
	await expect(page.getByRole('heading', { name: '4 Jul 2026' })).toBeVisible();
	await more.click();
	await page.getByRole('menuitem', { name: 'Share with a link' }).click();
	const sheet = page.getByRole('dialog', { name: 'Share flight' });
	await expect(sheet.getByRole('radio', { name: 'Only me' })).toBeChecked();
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

	// Deleting the flight says that it goes from the account at once, and it does.
	await more.click();
	await page.getByRole('menuitem', { name: 'Delete flight' }).click();
	const asking = page.getByRole('alertdialog', { name: 'Delete this flight?' });
	await expect(asking).toContainText(
		'deleted from your Venti account and your other devices at once'
	);
	await asking.getByRole('button', { name: 'Delete flight' }).click();
	await expect(page.getByRole('heading', { name: 'No flights yet' })).toBeVisible();
	await openAccount(page);

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
	await expect(page.getByRole('button', { name: 'Sign in', exact: true })).toBeVisible();

	// It is gone: nobody signs in to it any more.
	await field(page, 'Handle').fill(pilot.handle);
	await field(page, 'Password').fill(pilot.password);
	await page.getByRole('button', { name: 'Sign in', exact: true }).click();
	await expect(page.getByRole('alert')).toHaveText('The handle or the password is wrong.');

	// Nothing went wrong but what was meant to: the passwords the server
	// refused, which the browser notes as failed requests.
	expect(problems.filter((problem) => !/status of 40[13]/.test(problem))).toEqual([]);
});

test('the pilot’s own sites follow the account when the site is opened and comes back to the front', async ({
	page
}) => {
	await signIn(page);
	const asksForChanges = () =>
		page.waitForRequest((request) => request.url().includes('/v1/account/kept-sites/changes'));

	// Opened again, the site asks the server what changed, without being told to.
	const asked = asksForChanges();
	await page.reload();
	expect((await asked).headers().authorization).toMatch(/^Bearer venti_/);

	// So it does when the pilot comes back to the tab after a while. The
	// sync of the opening may still be under way, and one under way is not
	// started twice: the tab comes back until it is asked.
	let again = false;
	const listen = (request: { url(): string }) => {
		if (request.url().includes('/v1/account/kept-sites/changes')) again = true;
	};
	page.on('request', listen);
	await expect
		.poll(async () => {
			await page.evaluate(() => {
				// More than the minute that lies between two syncs nobody asked for.
				const later = Date.now() + 61_000;
				Date.now = () => later;
				document.dispatchEvent(new Event('visibilitychange'));
			});
			return again;
		})
		.toBe(true);
	page.off('request', listen);
});

test('a session that ended asks to sign in again', async ({ context, page, keep }) => {
	await signIn(page);

	// The session has ended on the server: the core says so when the account
	// is read, and the screen says what to do.
	await context.route('**/v1/account', (route) =>
		route.fulfill({
			status: 401,
			json: { error: { code: 'unauthorized', message: 'no session' } }
		})
	);
	await page.reload();
	await expect(page.getByRole('alert')).toHaveText(
		'This browser was signed out. Sign out here and sign in again.'
	);
	await keep('30-account-signed-out');
	await context.unroute('**/v1/account');

	await sections(page)
		.getByRole('link', { name: /^Settings/ })
		.click();
	await expect(
		page.locator('.settings-page').getByRole('link', { name: /^Account/ })
	).toContainText('Signed out');
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
		page.getByText('Venti takes new accounts by invite for now, and hands the codes out itself.')
	).toBeVisible();
	await keep('52-account-new-by-invite');

	// Closed: no form to fill in, and it says so. Signing in is as before.
	registration = 'closed';
	await page.reload();
	await page.getByRole('radio', { name: 'New account' }).check();
	await expect(
		page.getByText(
			'Venti takes no new accounts at the moment. An account you already have signs in as before.'
		)
	).toBeVisible();
	await expect(field(page, 'Handle')).toBeHidden();
	await expect(field(page, 'Invite code')).toBeHidden();
	await expect(page.getByRole('button', { name: 'Create account' })).toBeHidden();
	await keep('53-account-new-closed');
	await page.getByRole('radio', { name: 'Sign in' }).check();
	await expect(field(page, 'Handle')).toBeVisible();
	await expect(page.getByRole('button', { name: 'Sign in', exact: true })).toBeVisible();

	// A server older than the question says nothing, and may ask for a
	// code: the field is offered, as it always was.
	registration = undefined;
	await page.reload();
	await page.getByRole('radio', { name: 'New account' }).check();
	await expect(field(page, 'Invite code')).toBeVisible();
	await expect(
		page.getByText('An invite code is needed while Venti takes accounts by invite')
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
	await expect(
		page.getByText('This copy of the app is served without Venti behind it.')
	).toBeVisible();

	await page
		.locator('.settings-page')
		.getByRole('link', { name: /^Account/ })
		.click();
	await expect(
		page.getByText('This copy of the app is served without Venti behind it, so there is no account')
	).toBeVisible();
	await expect(page.getByRole('button', { name: 'Sign in', exact: true })).toBeHidden();
	await keep('32-account-no-server');
});

test('the pilot’s country and glider class are the account’s, and shown with the flights', async ({
	page,
	keep,
	problems
}) => {
	const handle = await signUp(page);
	await page.goto('/settings/account/');
	const profile = page.getByRole('region', { name: 'Shown with your flights' });
	const country = profile.getByRole('combobox', { name: 'Country' });
	const gliderClass = profile.getByRole('combobox', { name: 'Glider class' });
	await expect(country).toHaveValue('');
	await expect(gliderClass).toHaveValue('');

	// The country is chosen by its name, and its flag stands beside it.
	await country.selectOption({ label: 'Germany' });
	await expect(profile.getByRole('img', { name: 'Germany' })).toBeVisible();
	await gliderClass.selectOption({ label: 'EN B' });
	await expect(gliderClass).toHaveValue('b');
	await keep('110-account-profile');

	// Both are the server's: there after a reload.
	await page.reload();
	await expect(country).toHaveValue('DE');
	await expect(gliderClass).toHaveValue('b');

	// A flight brought in now takes the class; its row says it beside the glider.
	await page.goto('/settings/account/flights/');
	await page
		.locator('input[type=file]')
		.setInputFiles(asUpload('sample.igc', readFileSync(SAMPLE_FLIGHT, 'utf8')));
	await expect(page.getByRole('heading', { name: '4 Jul 2026' })).toBeVisible();
	await page.goto('/settings/account/flights/');
	const row = page.getByRole('main').locator('section.day li').first();
	await expect(row.locator('.glider')).toContainText('Sample Wing');
	await expect(row.locator('.glider')).toContainText('EN B');
	await keep('111-account-profile-flight');
	// The pilot's public page has it once the flight is public: the next test.

	// Cleared again: none.
	await page.goto('/settings/account/');
	await country.selectOption({ label: 'None' });
	await expect(profile.getByRole('img', { name: 'Germany' })).toBeHidden();
	await page.reload();
	await expect(country).toHaveValue('');
	expect(problems).toEqual([]);
});

test('who sees the pilot’s flights is the account’s, and a flight takes a glider class of its own', async ({
	page,
	keep,
	problems
}) => {
	// `signUp` switches "New flights are public" off: the flight comes in private.
	const handle = await signUp(page);
	const visibility = page.getByRole('region', { name: 'Who sees your flights' });
	const open = visibility.getByRole('switch', { name: 'New flights are public' });
	await expect(open).not.toBeChecked();
	await expect(visibility).toContainText('Switch off to keep new flights to yourself.');
	await page.goto('/settings/account/flights/');
	await page
		.locator('input[type=file]')
		.setInputFiles(asUpload('sample.igc', readFileSync(SAMPLE_FLIGHT, 'utf8')));
	await expect(page.getByRole('heading', { name: '4 Jul 2026' })).toBeVisible();

	// Brought in before the pilot set a class, the flight has none; its menu sets one.
	await page.getByRole('button', { name: 'More' }).filter({ visible: true }).click();
	await page.getByRole('menuitem', { name: 'Glider class' }).click();
	const classes = page.getByRole('dialog', { name: 'Glider class' });
	await expect(classes.getByRole('radio', { name: 'None' })).toBeChecked();
	await keep('111c-flight-glider-class');
	await classes.getByText('EN C', { exact: true }).click();
	await expect(classes).toBeHidden();
	await page.goto('/settings/account/flights/');
	const row = page.getByRole('main').locator('section.day li').first();
	await expect(row.locator('.glider')).toContainText('EN C');
	await expect(row.locator('.seen')).toBeHidden();

	// All at once: the core's question first, with the number of flights
	// where My flights has read them all, and its line after.
	await page.goto('/settings/account/');
	await visibility.getByRole('button', { name: 'Make all public' }).click();
	const asking = page.getByRole('alertdialog', {
		name: /^Make (your flight|all flights) public\?$/
	});
	await asking.getByRole('button', { name: 'Make all public' }).click();
	await expect(asking).toBeHidden();
	await expect(visibility.getByRole('status')).toHaveText(/public/);
	await keep('111b-account-who-sees');
	// My flights reads them again: the row says so beside the time, and the glider still reads.
	await page.goto('/settings/account/flights/');
	await expect(row.locator('.seen')).toHaveText('Public');
	await expect(row.locator('.glider')).toContainText('Sample Wing');
	await keep('111d-my-flights-public');
	await page.goto(`/pilots/${handle}/`);
	await expect(page.getByRole('main')).toContainText('Sample Wing · EN C');

	// And private again, at once: gone from the pilot's public page.
	await page.goto('/settings/account/');
	await visibility.getByRole('button', { name: 'Make all private' }).click();
	await page
		.getByRole('alertdialog', { name: /^Make (your flight|all flights) private\?$/ })
		.getByRole('button', { name: 'Make all private' })
		.click();
	await expect(visibility.getByRole('status')).toHaveText(/private/);
	await page.goto(`/pilots/${handle}/`);
	await expect(page.getByRole('main')).not.toContainText('Sample Wing');
	expect(problems).toEqual([]);
});

/**
 * A PNG of one colour, made here: as many pixels as a phone's photograph,
 * so that the page draws it anew at most 1024 pixels long before sending.
 */
function photograph(width: number, height: number): Buffer {
	const crcTable = Array.from({ length: 256 }, (_, n) => {
		let c = n;
		for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
		return c >>> 0;
	});
	const crc = (bytes: Buffer) => {
		let c = 0xffffffff;
		for (const byte of bytes) c = crcTable[(c ^ byte) & 0xff] ^ (c >>> 8);
		return (c ^ 0xffffffff) >>> 0;
	};
	const chunk = (kind: string, data: Buffer) => {
		const length = Buffer.alloc(4);
		length.writeUInt32BE(data.length);
		const body = Buffer.concat([Buffer.from(kind, 'ascii'), data]);
		const sum = Buffer.alloc(4);
		sum.writeUInt32BE(crc(body));
		return Buffer.concat([length, body, sum]);
	};
	const header = Buffer.alloc(13);
	header.writeUInt32BE(width, 0);
	header.writeUInt32BE(height, 4);
	header.set([8, 2, 0, 0, 0], 8);
	const row = Buffer.alloc(1 + width * 3);
	for (let x = 0; x < width; x++) row.set([230, 120, 90], 1 + x * 3);
	const pixels = Buffer.concat(Array.from({ length: height }, () => row));
	return Buffer.concat([
		Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
		chunk('IHDR', header),
		chunk('IDAT', deflateSync(pixels)),
		chunk('IEND', Buffer.alloc(0))
	]);
}

test('the pilot’s picture is the account’s, and whether they are among the pilots near a place', async ({
	page,
	keep,
	wide,
	problems
}) => {
	const handle = await signUp(page);
	// The bar across the top of a wide screen has the pilot's face before
	// the handle; a phone's tab bar has no account in it.
	const inTheBar = page.getByRole('navigation', { name: 'Sections' }).locator('a.account');
	if (wide) {
		await expect(inTheBar).toHaveText(new RegExp(`TP\\s*${handle}`));
		await expect(inTheBar.locator('.face')).toHaveText('TP');
	} else {
		await expect(inTheBar).toBeHidden();
	}
	const account = page.getByRole('region', { name: 'Account' });
	const picture = account.getByRole('group', { name: 'Picture' });
	// Without a picture: the initials of "Test Pilot".
	await expect(picture.locator('.face')).toHaveText('TP');
	await expect(picture.getByRole('button', { name: 'Remove picture' })).toBeHidden();
	await expect(account).toContainText('Everyone sees it, on your page and beside your flights.');

	// A photograph from a phone is drawn smaller in the browser and sent.
	const asked = page.waitForRequest(
		(request) => request.method() === 'PUT' && request.url().endsWith('/v1/account/picture')
	);
	await picture
		.locator('input[type=file]')
		.setInputFiles({ name: 'me.png', mimeType: 'image/png', buffer: photograph(2400, 1800) });
	const sent = await asked;
	expect(sent.postDataBuffer()!.length).toBeLessThan(2 * 1024 * 1024);
	const image = picture.locator('img');
	await expect(image).toHaveAttribute('src', /^\/v1\/pictures\/\w+$/);
	await expect.poll(() => image.evaluate((img: HTMLImageElement) => img.naturalWidth)).toBe(256);
	await keep('112-account-picture');

	// The server's: there after a reload.
	await page.reload();
	await expect(image).toHaveAttribute('src', /^\/v1\/pictures\/\w+$/);
	if (wide) {
		await expect(inTheBar.locator('img')).toHaveAttribute('src', /^\/v1\/pictures\/\w+$/);
		await expect(inTheBar).toHaveText(handle);
		await keep('112a-account-picture-in-the-bar');
	}

	// Shown among the pilots near a place until the pilot says otherwise.
	const near = page.getByRole('switch', { name: 'Show me among pilots near a place' });
	await expect(near).toBeChecked();
	await expect(page.getByText('From your public flights only.')).toBeVisible();
	await near.uncheck();
	await expect(near).not.toBeChecked();
	await expect(near).toBeEnabled();
	await page.reload();
	await expect(near).not.toBeChecked();
	await near.check();
	await expect(near).toBeChecked();

	// Removed: the initials again.
	await picture.getByRole('button', { name: 'Remove picture' }).click();
	await expect(image).toBeHidden();
	await expect(picture.locator('.face')).toHaveText('TP');
	await page.reload();
	await expect(picture.locator('.face')).toHaveText('TP');
	expect(problems).toEqual([]);
});
