/**
 * A browser with WebAssembly switched off, as Chrome has it where an
 * organisation does not allow JavaScript optimisation, and Safari in
 * Lockdown Mode. The core cannot run there, and nearly everything relies
 * on it: a popup over the whole window says so on every page, and cannot
 * be closed. The page asks before it starts the core's worker, so the
 * worker is never started and its own global needs no change.
 */

import type { Page } from '@playwright/test';
import { expect, test } from './helpers';

test.beforeEach(async ({ context }) => {
	await context.addInitScript(() => {
		Reflect.deleteProperty(globalThis, 'WebAssembly');
	});
});

/** Checks that the popup is there, says what it says, and covers the whole window. */
async function saysItNeedsWebAssembly(page: Page): Promise<void> {
	const popup = page.getByRole('dialog', { name: 'Venti needs WebAssembly' });
	await expect(popup).toBeVisible();
	await expect(popup.getByRole('heading', { level: 1 })).toHaveText('Venti needs WebAssembly');
	await expect(popup).toHaveAccessibleDescription(
		'The map, the forecast, your logbook and nearly everything else in Venti rely on it, and this browser has switched it off. Organisations often switch it off with JavaScript optimisation.'
	);
	await expect(popup.getByText(/^Open Venti in another browser: Safari, Firefox/)).toBeVisible();
	await expect(popup.getByText(/^Or allow JavaScript optimisation for this site/)).toBeVisible();
	// Neither the browser's own words nor the screens' state for a core that broke down.
	await expect(page.getByText('WebAssembly is not defined')).toHaveCount(0);
	await expect(page.getByText(/could not open its data/)).toHaveCount(0);
	await expect(page.getByRole('dialog')).toHaveCount(1);

	// Modal, so everything behind it is inert, and as large as the window.
	expect(await popup.evaluate((dialog) => dialog.matches(':modal'))).toBe(true);
	const viewport = page.viewportSize()!;
	expect(await popup.boundingBox()).toEqual({
		x: 0,
		y: 0,
		width: viewport.width,
		height: viewport.height
	});

	// The navigation is still in the page, but under the popup: where its
	// links are, the popup is what a click meets.
	const sections = page.locator('nav[aria-label="Sections"]');
	const box = (await sections.boundingBox())!;
	const middle = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
	expect(
		await page.evaluate(
			({ x, y }) => document.elementFromPoint(x, y)?.closest('dialog') !== null,
			middle
		)
	).toBe(true);
}

test('a browser without WebAssembly is told it needs it, over the whole window', async ({
	page,
	keep,
	problems
}) => {
	const workers: string[] = [];
	page.on('worker', (worker) => workers.push(worker.url()));

	await page.goto('/');
	await saysItNeedsWebAssembly(page);
	await expect(page).toHaveTitle('Venti');
	await keep('no-webassembly');

	// It cannot be closed: not by Escape, not by a click where the
	// navigation is, which leads nowhere.
	await page.keyboard.press('Escape');
	await page.keyboard.press('Escape');
	const flights = page
		.locator('nav[aria-label="Sections"] a[href="/flights"]')
		.filter({ visible: true })
		.first();
	const box = (await flights.boundingBox())!;
	await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
	expect(new URL(page.url()).pathname).toBe('/');
	await saysItNeedsWebAssembly(page);

	// Every page says the same, at once.
	for (const path of [
		'/map/',
		'/today/',
		'/settings/account/flights/',
		'/community/',
		'/settings/'
	]) {
		await page.goto(path);
		await saysItNeedsWebAssembly(page);
	}

	// The core was not started, so nothing broke down in the console.
	expect(workers.filter((url) => /\/workers\/worker-/.test(url))).toEqual([]);
	expect(problems).toEqual([]);
});
