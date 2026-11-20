/**
 * What a visitor sees first of what is flown on Venti: at the head of
 * everyone's flights and under an empty logbook, the best flights of the
 * day, or the newest where nobody shared one today, and the top of this
 * year's ranking. From the development mode's five public flights, all
 * of August and September 2026.
 */

import { expect, test } from './helpers';

test('everyone’s flights begin with the best of the day and the top of the year', async ({
	page,
	keep,
	problems
}) => {
	// A day on which two pilots flew.
	await page.clock.setFixedTime(new Date('2026-08-16T15:00:00'));
	await page.goto('/logbook/everyone/');

	const best = page.getByRole('list', { name: 'Best of today' }).getByRole('listitem');
	await expect(best).toHaveCount(2);
	await expect(best.first()).toContainText('11.98');
	await expect(page.getByRole('link', { name: 'All of today' })).toHaveAttribute(
		'href',
		/order=best&from=2026-08-16&to=2026-08-16/
	);
	const top = page.getByRole('list', { name: /^Top of \d{4}$/ }).getByRole('listitem');
	await expect(top).toHaveCount(3);
	await expect(top.first()).toContainText('Mara Muster');
	await expect(top.first()).toContainText('23.42');
	await keep('91-everyone-front');

	// The head stands over the plain list only: another order has its own.
	await page.getByRole('radio', { name: 'Best' }).check();
	await expect(page.getByRole('list', { name: 'Best of today' })).toHaveCount(0);
	await page.getByRole('radio', { name: 'Newest' }).check();
	await page.getByRole('link', { name: 'The whole ranking' }).click();
	await expect(page).toHaveURL(/\/logbook\/ranking\/$/);
	expect(problems).toEqual([]);
});

test('an empty logbook shows what others fly on Venti, and the way there', async ({
	page,
	keep
}) => {
	// Nobody flew today: the newest flights stand in.
	await page.clock.setFixedTime(new Date('2026-11-20T15:00:00'));
	await page.goto('/logbook/');
	await expect(page.getByRole('heading', { name: 'No flights yet' })).toBeVisible();

	await expect(
		page.getByText('Nobody has shared a flight of today yet. The newest:')
	).toBeVisible();
	await expect(page.getByRole('list', { name: 'Flown lately' }).getByRole('listitem')).toHaveCount(
		3
	);
	await expect(
		page.getByRole('list', { name: /^Top of \d{4}$/ }).getByRole('listitem')
	).toHaveCount(3);
	await keep('92-logbook-empty-others');

	await page.getByRole('link', { name: 'Everyone’s flights' }).click();
	await expect(page).toHaveURL(/\/logbook\/everyone\/$/);
	// On everyone's list the newest are the list itself: the head has the ranking alone.
	await expect(page.getByRole('list', { name: 'Flown lately' })).toHaveCount(0);
	await expect(page.getByRole('list', { name: /^Top of \d{4}$/ })).toBeVisible();
});
