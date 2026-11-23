/**
 * Foehn on Today, from Venti in its development mode, whose made-up
 * pressures put Bolzano 6.2 hPa above Innsbruck ("likely", south foehn)
 * and Klagenfurt and Salzburg level ("no sign"). Its clock stands still in
 * the evening of 18 November 2026, a quarter of an hour after the readings.
 *
 * Today's take-offs around the Tegernsee lie some 50 km from Innsbruck, so
 * the first pair speaks for them: one line over the take-offs, and the
 * view behind it. What the development server cannot be made to say on
 * demand, a server that gives no answer, the last test answers itself and
 * says so as made up.
 */

import type { Page } from '@playwright/test';
import { expect, test } from './helpers';

/** Keeps the screen light and dark. */
async function keepBoth(
	page: Page,
	keep: (name: string) => Promise<void>,
	number: number,
	name: string
) {
	await keep(`${number}-${name}`);
	await page.emulateMedia({ colorScheme: 'dark' });
	await keep(`${number + 1}-${name}-dark`);
	await page.emulateMedia({ colorScheme: 'light' });
}

const view = (page: Page) => page.getByRole('dialog', { name: 'Foehn' });

test('today says foehn in one line, and a tap opens the view with both pairs', async ({
	page,
	keep,
	problems
}) => {
	await page.goto('/');
	// The line, over the take-offs, in the danger's colour for "likely".
	const line = page.getByTestId('foehn-line');
	await expect(line).toHaveText('South foehn likely: 6 hPa higher at Bolzano than at Innsbruck');
	await expect(line).toHaveClass(/likely/);
	await expect(line.locator('svg').first()).toBeVisible();
	// It changes no take-off's status: those are the forecast's, as before.
	await expect(page.getByText(/^(Flyable|Marginal|Not flyable)$/).first()).toBeVisible({
		timeout: 30_000
	});
	await keepBoth(page, keep, 287, 'today-foehn');

	// The view: each pair with its words, its level, its reading's age and its chart.
	await line.click();
	const rows = view(page).getByTestId('foehn-row');
	await expect(rows).toHaveCount(2);
	const south = rows.nth(0);
	await expect(south.getByRole('heading', { name: 'Bolzano – Innsbruck' })).toBeVisible();
	await expect(south.locator('[data-level="likely"]')).toHaveText('Likely');
	await expect(south).toContainText(
		'South foehn likely: 6 hPa higher at Bolzano than at Innsbruck'
	);
	await expect(south).toContainText(/\d+ min ago/);
	const chart = south.getByTestId('foehn-chart');
	await expect(chart).toContainText('Last 24 hours');
	await expect(chart).toContainText('hPa');
	await expect(chart).toContainText('Higher at Bolzano: south foehn');
	await expect(chart).toContainText('Higher at Innsbruck: north foehn');
	await expect(chart.locator('path.line')).toHaveCount(1);
	await expect(chart.locator('line.limit')).toHaveCount(4);
	await expect(chart.locator('.dot')).toHaveCount(12);
	// The newest reading is at "likely", the oldest below every limit.
	await expect(chart.locator('.dot').last()).toHaveClass(/likely/);
	await expect(chart.locator('.dot').first()).toHaveClass(/none/);
	const level = rows.nth(1);
	await expect(level.getByRole('heading', { name: 'Klagenfurt – Salzburg' })).toBeVisible();
	// "No sign" is plain: neither a caution nor anything that reads as good.
	await expect(level.locator('[data-level="none"]')).toHaveText('No sign');
	await expect(level).toContainText('No sign of foehn in the pressure');
	// What the number means, what it does not say, and whose data it is.
	await expect(view(page)).toContainText('reduced to sea level');
	await expect(view(page).getByRole('link').first()).toBeVisible();
	await expect(view(page).getByText(/\bsafe\b/i)).toHaveCount(0);
	expect(problems).toEqual([]);
	await keepBoth(page, keep, 289, 'foehn-view');
	await view(page).getByRole('button', { name: 'Done' }).click();
	await expect(view(page)).toBeHidden();

	// The same view from the row at the page's foot.
	await page.locator('footer').getByRole('button', { name: 'Foehn', exact: true }).click();
	await expect(view(page).getByTestId('foehn-row')).toHaveCount(2);
});

test('without an answer today shows no line, and the view says why', async ({ page, keep }) => {
	// Made up: a server that is busy when asked for foehn.
	let busy = true;
	await page.route('**/v1/foehn', (route) =>
		busy ? route.fulfill({ status: 503, json: {} }) : route.continue()
	);
	await page.goto('/');
	await expect(page.getByText(/^(Flyable|Marginal|Not flyable)$/).first()).toBeVisible({
		timeout: 30_000
	});
	await expect(page.getByTestId('foehn-line')).toHaveCount(0);

	await page.locator('footer').getByRole('button', { name: 'Foehn', exact: true }).click();
	await expect(view(page).getByRole('alert')).toContainText('busy');
	await expect(view(page).getByTestId('foehn-row')).toHaveCount(0);
	await keep('291-foehn-no-answer');

	// The server answers again: the view fills, and the line stands over the take-offs.
	busy = false;
	await view(page).getByRole('button', { name: 'Try again' }).click();
	await expect(view(page).getByTestId('foehn-row')).toHaveCount(2);
	await view(page).getByRole('button', { name: 'Done' }).click();
	await expect(page.getByTestId('foehn-line')).toBeVisible();
});

test('the line and the view fit a narrow phone with large text', async ({ page, keep, wide }) => {
	test.skip(wide, 'A narrow phone: 360 px wide, text at 130 %.');
	await page.setViewportSize({ width: 360, height: 780 });
	await page.goto('/');
	await page.evaluate(() => (document.documentElement.style.fontSize = '130%'));
	const line = page.getByTestId('foehn-line');
	await expect(line).toBeVisible();
	/**
	 * What is drawn wider than the phone inside `within`, and the page itself
	 * if it can be moved sideways. The width is the phone's own: a page that
	 * is too wide makes the test browser's window wider with it.
	 */
	const tooWide = (within: string) =>
		page.evaluate(
			([within, width]) => {
				const wide = [...document.querySelectorAll(`${within} *`)]
					.filter((element) => {
						const box = element.getBoundingClientRect();
						return box.width > 0 && box.right > Number(width) + 1;
					})
					.map((element) => `${element.tagName.toLowerCase()}.${element.className}`);
				if (document.documentElement.scrollWidth > Number(width) + 1) wide.push('the page');
				return wide;
			},
			[within, 360] as const
		);
	expect(await tooWide('.foehn-line')).toEqual([]);
	await keep('292a-foehn-narrow-today');
	await line.click();
	await expect(view(page).getByTestId('foehn-row')).toHaveCount(2);
	expect(await tooWide('dialog[open]')).toEqual([]);
	await keep('292b-foehn-narrow-view');
});
