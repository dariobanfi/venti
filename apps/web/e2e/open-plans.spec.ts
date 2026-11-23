/**
 * Open plans on Community: the plans for everyone flying from an area near
 * the pilot, each a card, a plan's own page, posting one with its place on
 * a small map or from the take-offs near, changing and deleting it, and
 * another pilot's "I'm in". The server is the development mode's, which
 * keeps open plans in memory and has two made-up ones at the Musterberg,
 * near the region's middle, for 19 and 20 November 2026; its clock stands
 * still, a browser's does not.
 *
 * A test that posts does so from an account of its own, at a place named
 * by a random word, and deletes the plan again: the same test runs at four
 * widths on one server, and no other spec reads open plans. Nothing here
 * counts the plans near a place: it looks for a plan by its place's name.
 */

import type { Locator, Page } from '@playwright/test';
import { expect, signUp, test, withPasswordForm } from './helpers';

/** The browser's memory of the weather's place, as `src/lib/home/place.ts` keeps it. */
const remembered = (at: { lat: number; lon: number }) =>
	`localStorage.setItem('venti.weather-place', ${JSON.stringify(
		JSON.stringify({ at, kind: 'chosen', name: 'Made up' })
	)})`;

/** Community's section of the open plans near the pilot. */
const section = (page: Page): Locator => page.getByRole('region', { name: 'Open plans near' });

/** The card of an open plan, by its place's name. */
const card = (page: Page, place: string): Locator =>
	section(page)
		.getByRole('list', { name: 'Open plans near' })
		.locator('> li')
		.filter({ has: page.getByRole('link', { name: place, exact: true }) });

test('signed out, the open plans near the region’s middle, a plan’s page, and “I’m in” leads to an account', async ({
	page,
	keep,
	problems
}) => {
	// The day before the made-up plans, by the browser's clock.
	await page.clock.setFixedTime(new Date('2026-11-18T11:00:00Z'));
	await page.goto('/community/');
	const talstation = card(page, 'Musterberg, Talstation');
	await expect(talstation).toContainText('Tomorrow');
	await expect(talstation).toContainText('09:00');
	await expect(talstation).toContainText('Mara Muster');
	await expect(talstation).toContainText('2 seats free');
	await expect(talstation).toContainText('Two seats free from the valley');
	await expect(talstation).toContainText('Mara Muster and Jonas Beispiel');
	await expect(
		talstation.getByRole('list', { name: 'Who is in' }).getByRole('listitem')
	).toHaveCount(2);
	const company = card(page, 'Musterberg');
	await expect(company).toContainText('Looking for company');
	await expect(company).toContainText('Jonas Beispiel');
	// Posting needs an account: there is nothing to post with.
	await expect(section(page).getByRole('button', { name: 'New open plan' })).toHaveCount(0);
	await section(page).scrollIntoViewIfNeeded();
	await keep('276a-community-open-plans');

	// A plan's page says everything, and its author leads to their page.
	await talstation.getByRole('link', { name: 'Musterberg, Talstation' }).click();
	await expect(page).toHaveURL(/\/plans\/open\/\?id=a{26}$/);
	await expect(page.getByRole('heading', { level: 1, name: 'Open plan' })).toBeVisible();
	const facts = page.getByRole('region', { name: 'The plan', exact: true });
	await expect(facts).toContainText('Musterberg, Talstation');
	await expect(facts).toContainText('Tomorrow');
	await expect(facts).toContainText('2 seats free');
	await expect(facts.getByRole('link', { name: 'Mara Muster' })).toHaveAttribute(
		'href',
		/\/logbook\/everyone\/pilot\/?\?handle=mara$/
	);
	const who = page.getByRole('list', { name: 'Who is in' });
	await expect(who.getByRole('listitem')).toHaveText(['Mara Muster', 'Jonas Beispiel']);
	await expect(page.getByRole('button', { name: 'Report this plan' })).toHaveCount(0);
	await keep('276b-open-plan-signed-out');

	// Saying "I'm in" needs an account: it leads to one.
	await page.getByRole('main').getByRole('link', { name: "I'm in" }).click();
	await expect(page.getByRole('heading', { level: 1, name: 'Account' })).toBeVisible();

	// A plan that is not there says so in the core's words.
	await page.goto('/plans/open/?id=zzzzzzzzzzzzzzzzzzzzzzzzzz');
	await expect(page.getByRole('main').getByRole('alert')).not.toBeEmpty();
	expect(problems.filter((line) => !/404/.test(line))).toEqual([]);
});

test('a pilot posts an open plan from a take-off near, another says “I’m in”, and its author changes and deletes it', async ({
	page,
	browser,
	keep,
	mapDrawn,
	problems
}) => {
	// In the valley, where the development server has take-offs.
	await page.addInitScript(remembered({ lat: 47.682, lon: 11.765 }));
	await signUp(page);
	await page.goto('/community/');
	await section(page).getByRole('button', { name: 'New open plan' }).click();
	const form = page.getByRole('dialog', { name: 'New open plan' });
	await expect(form.getByText(/^Everyone can read an open plan/)).toBeVisible();

	// The place from the take-offs near, then named the test's own way.
	const takeoff = form.getByRole('combobox', { name: 'Take-off', exact: true });
	await expect(takeoff.locator('option:not([disabled])').first()).toBeAttached();
	const first = await takeoff.locator('option:not([disabled])').first().getAttribute('value');
	await takeoff.selectOption(first!);
	await expect(form.getByRole('textbox', { name: 'Where' })).not.toHaveValue('');
	const place = `Testwiese ${Math.random().toString(36).slice(2, 8)}`;
	await form.getByRole('textbox', { name: 'Where' }).fill(place);
	await form.getByRole('switch', { name: 'Set a time' }).check();
	await form.getByLabel('Time', { exact: true }).fill('09:30');
	await form.getByRole('radio', { name: 'Seats free' }).check();
	await form.getByRole('combobox', { name: 'Seats', exact: true }).selectOption('3');

	// Too long a note is refused in the core's words.
	await form.getByRole('textbox', { name: 'Note' }).fill('x'.repeat(281));
	await form.getByRole('button', { name: 'Post' }).click();
	await expect(form.getByRole('alert')).toHaveText('The note has room for 280 characters.');
	await form.getByRole('textbox', { name: 'Note' }).fill('A made-up open plan of a test.');
	await expect(form.getByRole('alert')).toHaveCount(0);
	await mapDrawn();
	await keep('276c-open-plan-form');
	await form.getByRole('button', { name: 'Post' }).click();
	await expect(form).toBeHidden();

	// The plan is a card near the pilot, with the pilot in.
	const posted = card(page, place);
	await expect(posted).toContainText('3 seats free');
	await expect(posted).toContainText('09:30');
	await expect(posted).toContainText('Only you so far');
	await expect(posted.getByRole('button', { name: "I'm in" })).toHaveAttribute(
		'aria-pressed',
		'true'
	);
	await posted.scrollIntoViewIfNeeded();
	await keep('276d-community-open-plan-posted');
	await posted.getByRole('link', { name: place }).click();
	await expect(page).toHaveURL(/\/plans\/open\/\?id=[a-z0-9]{26}$/);
	const address = page.url();

	// Another pilot, in a browser of their own, says "I'm in", and could report it.
	const others = await browser.newContext();
	await withPasswordForm(others);
	const other = await others.newPage();
	await signUp(other);
	await other.goto(address);
	await expect(other.getByRole('button', { name: 'Report this plan' })).toBeVisible();
	await expect(other.getByRole('button', { name: 'Change the plan' })).toHaveCount(0);
	await other.getByRole('button', { name: "I'm in" }).click();
	await expect(other.getByRole('button', { name: "I'm out" })).toBeVisible();
	await expect(other.getByRole('list', { name: 'Who is in' }).getByRole('listitem')).toHaveText([
		'You',
		'Test Pilot'
	]);
	await others.close();

	// Its author sees who came in, changes it to looking for company, and deletes it.
	await page.reload();
	await expect(page.getByRole('list', { name: 'Who is in' }).getByRole('listitem')).toHaveText([
		'You',
		'Test Pilot'
	]);
	await keep('276e-open-plan-mine');
	await page.getByRole('button', { name: 'Change the plan' }).click();
	const change = page.getByRole('dialog', { name: 'Change the open plan' });
	await expect(change.getByRole('textbox', { name: 'Where' })).toHaveValue(place);
	await change.getByRole('radio', { name: 'Looking for company' }).check();
	await change.getByRole('button', { name: 'Save' }).click();
	await expect(change).toBeHidden();
	await expect(page.getByRole('region', { name: 'The plan', exact: true })).toContainText(
		'Looking for company'
	);
	await page.getByRole('button', { name: 'Delete the plan' }).click();
	await page
		.getByRole('alertdialog', { name: 'Delete this plan?' })
		.getByRole('button', { name: 'Delete the plan' })
		.click();
	await expect(page).toHaveURL(/\/community\/$/);
	await expect(section(page)).not.toContainText('Loading…');
	await expect(card(page, place)).toHaveCount(0);
	expect(problems).toEqual([]);
});
