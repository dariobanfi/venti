/**
 * No page scrolls sideways at a desktop's middle widths: a table too wide
 * for its card scrolls inside it, and a side column goes under the table
 * where both do not fit. Run in one desktop project; the widths are set
 * here.
 */

import type { Page } from '@playwright/test';
import { expect, signUp, test } from './helpers';

/** How far the page is wider than the window, and what sticks out furthest. */
async function sideways(page: Page) {
	return page.evaluate(() => {
		const width = window.innerWidth;
		const over = document.documentElement.scrollWidth - width;
		const out = [...document.querySelectorAll<HTMLElement>('body *')]
			.map((element) => ({ element, right: element.getBoundingClientRect().right }))
			.filter(({ right }) => right > width + 1)
			.sort((a, b) => b.right - a.right)
			.slice(0, 6)
			.map(
				({ element, right }) =>
					`${element.tagName.toLowerCase()}.${[...element.classList].join('.')} ${Math.round(right)}`
			);
		return { over, out };
	});
}

const PAGES = [
	{ path: '/flights/', ready: (page: Page) => page.locator('table').first() },
	{ path: '/ranking/', ready: (page: Page) => page.locator('table').first() },
	{
		path: '/pilots/demo/',
		ready: (page: Page) => page.locator('table').first()
	},
	// The home page, with Today first: ready once a take-off's status is in.
	{ path: '/', ready: (page: Page) => page.getByText(/^(Flyable|Marginal|Not flyable)$/).first() },
	{
		path: '/settings/account/flights/',
		ready: (page: Page) => page.getByRole('heading', { level: 1, name: 'My flights' })
	}
];

for (const width of [900, 1024, 1280]) {
	test(`no page is wider than a window ${width} px wide`, async ({ page, keep, problems }) => {
		await page.setViewportSize({ width, height: 800 });
		await signUp(page);
		for (const { path, ready } of PAGES) {
			await page.goto(path);
			await expect(ready(page)).toBeVisible({ timeout: 30_000 });
			const found = await sideways(page);
			expect(found.over, `${path}: ${found.out.join(', ')}`).toBeLessThanOrEqual(0);
			if (path === '/ranking/') await keep(`181-no-sideways-${width}`);
			if (path !== '/flights/') continue;
			await keep(`180-no-sideways-${width}`);

			// The table's last column is reached by scrolling the table's card.
			const card = page.locator('.card.wide').first();
			const reached = await card.evaluate((element) => {
				element.scrollLeft = element.scrollWidth;
				const last = element.querySelector('thead th:last-child')!.getBoundingClientRect();
				return last.right <= element.getBoundingClientRect().right + 1;
			});
			expect(reached, path).toBe(true);
			await card.evaluate((element) => (element.scrollLeft = 0));

			// The best of today and the season: beside the table where both fit, else under it.
			const side = page.getByRole('complementary', {
				name: 'Best of today and the top of the season'
			});
			const sideTop = (await side.boundingBox())!.y;
			const tableBottom = (await card.boundingBox())!.y + (await card.boundingBox())!.height;
			if (width < 1280) expect(sideTop).toBeGreaterThan(tableBottom);
			else expect(sideTop).toBeLessThan(tableBottom);
		}
		expect(problems).toEqual([]);
	});
}
