/**
 * The screens of the map's one layers sheet and of pilots among each other
 * on a narrow phone with large text: 360 px wide with the root font at
 * 130 %, set on every document as it loads. Phone projects only, so that
 * the screenshots are kept in light and in dark. On every screen nothing is
 * drawn wider than the screen, outside a row that scrolls sideways by
 * design. The demo account is only read: the alarm's form is closed
 * unsaved.
 */

import type { Page } from '@playwright/test';
import {
	BETWEEN_THE_MOUNTAINS,
	expect,
	mapTabDrawn,
	mapTool,
	metric,
	openCredits,
	openLayers,
	rowsOf,
	showTheValley,
	test
} from './helpers';

const DEMO = { handle: 'demo', password: 'demo-pilot-password' };

const field = (page: Page, name: string) => page.getByRole('textbox', { name, exact: true });

/** What the page draws wider than the screen, outside a row that scrolls sideways. */
const tooWide = (page: Page) =>
	page.evaluate(() => {
		const scrolls = (element: Element | null): boolean =>
			element !== null &&
			(['auto', 'scroll'].includes(getComputedStyle(element).overflowX) ||
				scrolls(element.parentElement));
		return [...document.querySelectorAll('body *')]
			.filter((element) => {
				if (element.closest('.maplibregl-map')) return false;
				// What measures and is never seen.
				if (getComputedStyle(element).visibility === 'hidden') return false;
				const box = element.getBoundingClientRect();
				return box.width > 1 && box.right > innerWidth + 1 && !scrolls(element.parentElement);
			})
			.map((element) => `${element.tagName.toLowerCase()}.${element.className}`);
	});

test('tonight’s screens fit a narrow phone with large text', async ({
	context,
	page,
	mapDrawn,
	keep,
	wide,
	problems
}) => {
	test.skip(wide, 'A narrow phone: 360 px wide, text at 130 %.');
	test.setTimeout(120_000);
	await page.setViewportSize({ width: 360, height: 780 });
	await page.addInitScript(() => {
		const large = () => (document.documentElement.style.fontSize = '130%');
		if (document.documentElement) large();
		else document.addEventListener('DOMContentLoaded', large);
	});
	const fits = async (name: string) => {
		await keep(`280-narrow-${name}`);
		expect.soft(await tooWide(page), name).toEqual([]);
	};

	await page.goto('/settings/account/');
	await field(page, 'Handle').fill(DEMO.handle);
	await field(page, 'Password').fill(DEMO.password);
	await page.getByRole('button', { name: 'Sign in', exact: true }).click();
	await expect(metric(page, 'Handle')).toHaveText(`@${DEMO.handle}`);

	// The map: its chips, the layers sheet, the credits.
	await context.grantPermissions(['geolocation']);
	await context.setGeolocation(BETWEEN_THE_MOUNTAINS);
	await page.goto('/map/');
	await mapTabDrawn(page, mapDrawn);
	await fits('a-map-chips');
	// No name in the navigation is cut short: a long one takes a second line
	// under its icon, inside its link.
	const cut = await page
		.getByRole('navigation', { name: 'Sections' })
		.locator('li:visible .name')
		.evaluateAll((names) =>
			names
				.filter((name) => {
					const own = name.getBoundingClientRect();
					const link = name.closest('a')!.getBoundingClientRect();
					return (
						name.scrollWidth > name.clientWidth + 1 ||
						own.top < link.top - 0.5 ||
						own.bottom > link.bottom + 0.5
					);
				})
				.map((name) => name.textContent)
		);
	expect.soft(cut, 'names cut in the navigation').toEqual([]);
	const sheet = await openLayers(page);
	await fits('b-layers');
	await sheet.getByRole('group', { name: 'Lists and plans' }).scrollIntoViewIfNeeded();
	await fits('c-layers-tools');
	await sheet.getByRole('button', { name: 'Done' }).click();
	await openCredits(page);
	await fits('d-credits');

	// The planner's More menu.
	await mapTool(page, 'Plan a route');
	const card = page.getByRole('region', { name: 'Plan a route' });
	await card.getByRole('button', { name: 'More' }).click();
	await expect(page.getByRole('menuitem', { name: 'Import GPX' })).toBeVisible();
	await fits('e-planner-more');
	await page.keyboard.press('Escape');
	await card.getByRole('button', { name: 'Close' }).click();

	// A take-off's alarm for pilots in the air, closed unsaved.
	await showTheValley(page, mapDrawn, 2);
	await mapTool(page, 'Flying sites');
	await page
		.getByRole('dialog', { name: 'Flying sites' })
		.getByRole('listitem')
		.filter({ hasText: /Take-off\s*No more is known/ })
		.first()
		.getByRole('button')
		.click();
	const site = page.getByRole('dialog').filter({ has: page.locator('[data-site-air]') });
	await expect(site.locator('[data-site-air]')).toBeVisible();
	await fits('f-site-in-the-air');
	await site.getByRole('button', { name: 'Alert me' }).click();
	const form = page.getByRole('dialog', { name: 'Alert me' });
	await form
		.getByRole('group', { name: 'Kind' })
		.getByRole('button', { name: 'Pilots in the air' })
		.click();
	await expect(form.getByRole('textbox', { name: 'Pilots in the air, at least' })).toBeVisible();
	await fits('g-alarm-pilots');
	await form.getByRole('button', { name: 'Done' }).click();
	await expect(form).toBeHidden();

	// Everyone's flights with its Filter open, and the Following choice.
	await page.goto('/logbook/everyone/');
	await expect(rowsOf(page, 'Everyone’s flights').first()).toBeVisible();
	await page.getByRole('button', { name: 'Filter', exact: true }).click();
	await expect(page.getByLabel('From')).toBeVisible();
	await fits('h-everyone-filter');
	await page.getByRole('radio', { name: 'Following' }).check();
	await expect(rowsOf(page, 'Following').first()).toBeVisible();
	await fits('i-feed');

	// A flight with its cheers and comments, in a sheet on a phone.
	await rowsOf(page, 'Following').first().locator('a.flight').click();
	await expect(page).toHaveURL(/\/logbook\/everyone\/flight\/\?id=/);
	await fits('j-flight');
	await page.getByRole('button', { name: /cheered|comment/i }).click();
	const said = page.getByRole('region', { name: 'Comments' });
	await expect(said.getByRole('textbox', { name: 'Write a comment' })).toBeVisible();
	await fits('k-flight-comments');
	await said.getByRole('textbox', { name: 'Write a comment' }).scrollIntoViewIfNeeded();
	await fits('l-flight-comment-field');

	// The home with its Following section.
	await page.goto('/');
	const home = page.getByRole('region', { name: 'Following' });
	await expect(home).toContainText(/cheered/);
	await home.scrollIntoViewIfNeeded();
	await fits('m-home-following');

	// A pilot's page with Follow.
	await page.goto('/logbook/everyone/pilot/?handle=mara');
	await expect(page.getByRole('button', { name: 'Following' })).toBeVisible();
	await fits('n-pilot-follow');

	// What the pilot was told.
	await page.goto('/settings/account/notifications/');
	await expect(
		page.getByRole('list', { name: 'Notifications' }).getByRole('listitem')
	).not.toHaveCount(0);
	await fits('o-notifications');

	// The alarms with "Notify this browser".
	await page.goto('/settings/account/alerts/');
	const notify = page.getByRole('switch', { name: 'Notify this browser' });
	await notify.scrollIntoViewIfNeeded();
	await fits('p-alerts-notify');

	expect(problems).toEqual([]);
});
