/**
 * What a pilot keeps on their account besides flights: the take-offs they
 * mark as their own, which follow the account to their other browsers, and
 * "Programs with access", the tokens they give a logger or a script.
 *
 * The server is the development mode's. Its demo account, which has one
 * made-up token ("Demo logger"), is not touched: tests run side by side and
 * again on the same server, so each makes an account of its own.
 */

import type { Page } from '@playwright/test';
import { expect, metric, test } from './helpers';

/** A handle nobody has used on this server: a letter, then letters and digits. */
function newHandle(): string {
	return `pilot${Math.random().toString(36).slice(2, 10)}`;
}

const PASSWORD = 'correct horse battery staple';

const field = (page: Page, name: string) => page.getByRole('textbox', { name });
const sections = (page: Page) => page.getByRole('navigation', { name: 'Sections' });

/** Makes an account on the server and leaves the page on it, signed in. */
async function createAccount(page: Page, handle: string): Promise<void> {
	await page.goto('/settings/account/');
	await page.getByRole('radio', { name: 'New account' }).check();
	await field(page, 'Handle').fill(handle);
	await field(page, 'Name shown to others').fill('Test Pilot');
	await field(page, 'Password').fill(PASSWORD);
	await page.getByRole('button', { name: 'Create account' }).click();
	await expect(metric(page, 'Handle')).toHaveText(`@${handle}`);
}

test('a kept take-off goes to the pilot’s account and comes back in a browser that held nothing', async ({
	page,
	browser,
	baseURL,
	keep,
	problems
}) => {
	const NAME = 'Fluggelände Jägerkamp/Nagelspitz';
	const handle = newHandle();
	await createAccount(page, handle);

	// Signed in, a take-off is kept as ever, on its page.
	await page.goto(
		`/logbook/everyone/place/?lat=47.6791413&lon=11.9051032&site=${encodeURIComponent('osm-free-flying:way/1080986173')}`
	);
	await page.getByRole('button', { name: 'Keep as my take-off' }).click();
	await expect(page.getByRole('button', { name: 'Kept' })).toHaveAttribute('aria-pressed', 'true');

	// The list says, in the core's sentence, that it is on the account: the
	// sync that follows keeping it has run.
	await sections(page).getByRole('link', { name: 'Settings', exact: true }).click();
	const own = page.getByRole('list', { name: 'Your take-offs' });
	await expect(own.getByRole('listitem')).toHaveCount(1);
	await expect(own.getByRole('link', { name: new RegExp(NAME) })).toBeVisible();
	await expect(page.getByText(/^Kept on your Venti account and on this device\./)).toBeVisible();
	await keep('168-take-offs-on-the-account');

	// Another browser, which holds nothing, signs in to the same account
	// and has the take-off from it.
	const elsewhere = await browser.newContext({ baseURL });
	try {
		const second = await elsewhere.newPage();
		await second.goto('/settings/');
		await expect(second.getByRole('list', { name: 'Your take-offs' })).toHaveText('None kept');
		await expect(second.getByText(/^Kept on this device only: sign in/)).toBeVisible();
		await second
			.locator('.settings-page')
			.getByRole('link', { name: /^Account/ })
			.click();
		await field(second, 'Handle').fill(handle);
		await field(second, 'Password').fill(PASSWORD);
		await second.getByRole('button', { name: 'Sign in' }).click();
		await expect(metric(second, 'Handle')).toHaveText(`@${handle}`);
		await sections(second).getByRole('link', { name: 'Settings', exact: true }).click();
		const theirs = second.getByRole('list', { name: 'Your take-offs' });
		await expect(theirs.getByRole('link', { name: new RegExp(NAME) })).toBeVisible();
		await expect(theirs.getByRole('listitem')).toHaveCount(1);
		await expect(
			second.getByText(/^Kept on your Venti account and on this device\./)
		).toBeVisible();

		// Forgotten there, it goes from the account, and from the first
		// browser with its next sync.
		await theirs.getByRole('button', { name: `Forget ${NAME}` }).click();
		await expect(theirs).toHaveText('None kept');
		// The account has heard of it once the note no longer speaks of the next sync.
		await expect(
			second.getByText(/^Kept on your Venti account and on this device\./)
		).toBeVisible();
	} finally {
		await elsewhere.close();
	}
	await page.goto('/settings/account/');
	await page.getByRole('button', { name: 'Sync now' }).click();
	await sections(page).getByRole('link', { name: 'Settings', exact: true }).click();
	await expect(own).toHaveText('None kept');
	expect(problems).toEqual([]);
});

test('a pilot gives a program access, sees its secret once, and withdraws the access', async ({
	context,
	page,
	keep,
	problems
}) => {
	await context.grantPermissions(['clipboard-read', 'clipboard-write']);
	// Whatever the page or its worker writes to the console: the secret is never in it.
	const logged: string[] = [];
	page.on('console', (message) => logged.push(message.text()));
	page.on('worker', (worker) => worker.on('console', (message) => logged.push(message.text())));

	await createAccount(page, newHandle());

	// The account leads to the page, under the core's name for it.
	await page.getByRole('link', { name: 'Programs with access' }).click();
	await expect(page).toHaveTitle('Programs with access · Venti');
	await expect(page.getByRole('heading', { level: 1, name: 'Programs with access' })).toBeVisible();
	await expect(page.getByText(/^Let a logger, a script or another app/)).toBeVisible();
	await expect(page.getByText('No program has access to your account.')).toBeVisible();
	await expect(page.getByText(/^No program can change your password/)).toBeVisible();

	// The form, in the core's words; what cannot be sent is refused in them
	// before the server is asked.
	await page.getByRole('button', { name: 'Give access' }).click();
	const form = page.getByRole('dialog', { name: 'Give access' });
	const give = form.getByRole('button', { name: 'Give access' });
	await expect(form.getByText('What it may do')).toBeVisible();
	await expect(form.getByRole('switch')).toHaveCount(12);
	await expect(form.getByText(/^Leave empty for access that does not end/)).toBeVisible();
	await expect(form.getByText(/^Asked for so that nobody who picks up this device/)).toBeVisible();
	await give.click();
	await expect(form.getByRole('alert')).toHaveText(/^Give the program a name/);
	await form.getByRole('textbox', { name: 'Name of the program' }).fill('My logger');
	await give.click();
	await expect(form.getByRole('alert')).toHaveText('Choose at least one thing the program may do.');
	await form.getByRole('switch', { name: /^Flights: write/ }).check();
	await form.getByRole('switch', { name: /^Live tracking: send/ }).check();
	await form.getByRole('textbox', { name: 'Name of the program' }).scrollIntoViewIfNeeded();
	await keep('169-programs-form');
	// The same once in the dark, whatever the appearance the test runs in.
	const dark = await page.evaluate(() => matchMedia('(prefers-color-scheme: dark)').matches);
	await page.emulateMedia({ colorScheme: 'dark' });
	await keep('169-programs-form-dark');
	await page.emulateMedia({ colorScheme: dark ? 'dark' : 'light' });

	await give.click();
	await expect(form.getByRole('alert')).toHaveText('Enter your password.');

	// A wrong password is the server's to refuse, in the core's words.
	await form.getByRole('textbox', { name: 'Your password' }).fill('not the password at all');
	await give.click();
	await expect(form.getByRole('alert')).toHaveText('The password is wrong.');
	await form.getByRole('textbox', { name: 'Your password' }).fill(PASSWORD);
	// Given: the secret, this once, with what the core says beside it.
	await give.click();
	await expect(page.getByRole('heading', { level: 2, name: 'My logger' })).toBeVisible();
	await expect(page.getByText('Copy it now: Venti does not show it again.')).toBeVisible();
	await expect(page.getByText(/^Paste it into the program where it asks/)).toBeVisible();
	const secret = (await page.locator('p.secret').innerText()).trim();
	expect(secret).toMatch(/^venti_pat_\w+$/);
	await page.getByRole('button', { name: 'Copy', exact: true }).click();
	await expect(page.getByRole('status')).toHaveText('Copied.');
	expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(secret);
	await keep('170-programs-secret');

	// It is in no address and in nothing the browser keeps for the page.
	expect(page.url()).not.toContain(secret);
	const keptByThePage = await page.evaluate(() =>
		JSON.stringify([{ ...localStorage }, { ...sessionStorage }, history.state])
	);
	expect(keptByThePage).not.toContain('venti_pat_');

	// The page is not left for another before the pilot says it is copied.
	await sections(page).getByRole('link', { name: 'Settings', exact: true }).click();
	await expect(page.getByRole('button', { name: 'I have copied it' })).toBeFocused();
	await expect(page).toHaveURL(/\/settings\/account\/programs\/?$/);
	await page.getByRole('button', { name: 'I have copied it' }).click();

	// The list has it, with what it may do and its dates, and the secret is gone.
	const list = page.getByRole('list', { name: 'Programs with access' });
	const row = list.getByRole('listitem').filter({ hasText: 'My logger' });
	await expect(list.getByRole('listitem')).toHaveCount(1);
	await expect(row).toContainText('Flights: write · Live tracking: send');
	await expect(row).toContainText(/Made \d+ \w{3} \d{4} · Last used never ·\s+Ends never/);
	await expect(page.getByText(secret)).toHaveCount(0);
	await expect(page.locator('p.secret')).toHaveCount(0);
	await keep('171-programs-list');

	// Withdrawing asks first, in the core's words; "Keep access" leaves it.
	await row.getByRole('button', { name: 'Withdraw access' }).click();
	const question = page.getByRole('alertdialog', { name: 'Withdraw the access of “My logger”?' });
	await expect(question).toBeVisible();
	await keep('172-programs-withdraw');
	await question.getByRole('button', { name: 'Keep access' }).click();
	await expect(question).toBeHidden();
	await expect(list.getByRole('listitem')).toHaveCount(1);
	await row.getByRole('button', { name: 'Withdraw access' }).click();
	await question.getByRole('button', { name: 'Withdraw access' }).click();
	await expect(page.getByText('No program has access to your account.')).toBeVisible();
	await expect(list).toHaveCount(0);
	await keep('173-programs-none');

	expect(logged.join('\n')).not.toContain(secret);
	// The wrong password was refused by the server, which the browser notes.
	expect(problems.filter((problem) => !/status of 40[13]/.test(problem))).toEqual([]);
});
