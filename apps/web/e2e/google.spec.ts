/**
 * Signing in with Google, against the development server's made-up Google
 * (`made-up-google.ts`): a pilot new to Venti continues with Google,
 * chooses a handle and is signed in; signed out and back, the same Google
 * account signs in at once, without the handle step.
 */

import { expect, metric, test } from './helpers';
import { madeUpGoogle } from './made-up-google';

test('a pilot continues with Google, chooses a handle, and next time is signed in at once', async ({
	page,
	keep,
	problems
}) => {
	// A Google account new to the server, every run.
	const email = `ui.test.${Math.random().toString(36).slice(2, 8)}@example.com`;
	await madeUpGoogle(page, { email, name: 'UI Test' });
	await page.goto('/settings/account/');

	// Google's button first; the handle and password form waits behind it.
	const google = page.getByRole('button', { name: 'Continue with Google' });
	await expect(google).toBeVisible();
	const withPassword = page.getByRole('button', { name: 'Sign in with a handle and password' });
	await expect(withPassword).toHaveAttribute('aria-expanded', 'false');
	await expect(page.getByRole('textbox', { name: 'Password' })).toHaveCount(0);
	await keep('250-account-google');
	await page.emulateMedia({ colorScheme: 'dark' });
	await keep('251-account-google-dark');
	await page.emulateMedia({ colorScheme: 'light' });
	await withPassword.click();
	await expect(page.getByRole('textbox', { name: 'Password' })).toBeVisible();
	await withPassword.click();
	await expect(page.getByRole('textbox', { name: 'Password' })).toHaveCount(0);

	// A first sign-in: the handle step, with the server's suggestion and the name from Google.
	await google.click();
	await expect(page.getByRole('heading', { name: 'Choose your handle' })).toBeVisible();
	const handle = page.getByRole('textbox', { name: 'Handle' });
	await expect(handle).toHaveValue(/^ui_test/);
	const chosen = await handle.inputValue();
	await expect(page.getByText(`@${chosen} is free.`, { exact: false })).toBeVisible();
	await expect(page.getByRole('textbox', { name: 'Name shown to others' })).toHaveValue('UI Test');
	await keep('252-google-handle');
	await page.emulateMedia({ colorScheme: 'dark' });
	await keep('253-google-handle-dark');
	await page.emulateMedia({ colorScheme: 'light' });
	await page.getByRole('button', { name: 'Create account' }).click();

	// Signed in, with the Google account's email shown to the pilot.
	await expect(metric(page, 'Handle')).toHaveText(`@${chosen}`);
	const linked = page.getByRole('region', { name: 'Google' });
	await expect(linked).toContainText(email);
	await expect(linked.getByRole('button', { name: 'Unlink Google' })).toBeVisible();
	await keep('254-account-google-signed-in');
	await page.emulateMedia({ colorScheme: 'dark' });
	await keep('255-account-google-signed-in-dark');
	await page.emulateMedia({ colorScheme: 'light' });

	// Signed out and back: the same Google account signs in at once.
	await page.getByRole('button', { name: 'Sign out' }).click();
	const asking = page.getByRole('alertdialog', { name: 'Sign out?' });
	await asking.getByRole('button', { name: 'Sign out' }).click();
	await expect(google).toBeVisible();
	await google.click();
	await expect(metric(page, 'Handle')).toHaveText(`@${chosen}`);
	await expect(page.getByRole('heading', { name: 'Choose your handle' })).toHaveCount(0);
	await expect(page.getByRole('region', { name: 'Google' })).toContainText(email);

	expect(problems).toEqual([]);
});
