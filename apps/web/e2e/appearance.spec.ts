/**
 * Light, dark or the system's, chosen in Settings and kept in this
 * browser: applied before the page is drawn, and the system's again once
 * "Automatic" is chosen.
 */

import type { Page } from '@playwright/test';
import { expect, test } from './helpers';

/** The page's background as drawn. */
const background = (page: Page) =>
	page.evaluate(() => getComputedStyle(document.body).backgroundColor);

/** The browser bar's colour that holds now. */
const barColour = (page: Page) =>
	page.evaluate(
		() =>
			[...document.querySelectorAll<HTMLMetaElement>('meta[name="theme-color"]')].find(
				(meta) => matchMedia(meta.media).matches
			)?.content
	);

test('the pilot chooses dark in a light system, keeps it after a reload, and goes back to the system’s', async ({
	page,
	keep,
	problems
}) => {
	await page.emulateMedia({ colorScheme: 'light' });
	await page.goto('/settings/');
	const appearance = page.getByRole('radiogroup', { name: 'Appearance' });
	await expect(appearance.getByRole('radio', { name: 'Automatic' })).toBeChecked();
	const light = await background(page);
	const lightBar = await barColour(page);

	await appearance.getByText('Dark', { exact: true }).click();
	await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
	await expect.poll(() => background(page)).not.toBe(light);
	const dark = await background(page);
	expect(await barColour(page)).not.toBe(lightBar);
	await keep('23a-settings-dark');

	// Kept: dark from the first paint after a reload, before the app has started.
	await page.reload();
	await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
	await expect(appearance.getByRole('radio', { name: 'Dark' })).toBeChecked();
	expect(await background(page)).toBe(dark);

	// Automatic follows the system, whichever it is.
	await appearance.getByText('Automatic', { exact: true }).click();
	await expect(page.locator('html')).not.toHaveAttribute('data-theme', /./);
	await expect.poll(() => background(page)).toBe(light);
	expect(await barColour(page)).toBe(lightBar);
	await page.emulateMedia({ colorScheme: 'dark' });
	await expect.poll(() => background(page)).toBe(dark);
	await page.reload();
	await expect(page.locator('html')).not.toHaveAttribute('data-theme', /./);
	expect(await background(page)).toBe(dark);
	expect(problems).toEqual([]);
});
