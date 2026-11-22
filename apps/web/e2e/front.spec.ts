/**
 * What a visitor sees first of what is flown on Venti: at the head of
 * everyone's flights and under an empty logbook, the best flights of the
 * day, or the newest where nobody shared one today, and the top of this
 * year's ranking. From the development mode's public flights, the newest of
 * August and September 2026.
 */

import { expect, test } from './helpers';

test('everyone’s flights begin with the best of the day and the top of the year', async ({
	page,
	keep,
	wide,
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
	// A phone has the first three, a wide screen's side column the first ten: all six of them.
	const top = page.getByRole('list', { name: /^Top of \d{4}$/ }).getByRole('listitem');
	await expect(top).toHaveCount(wide ? 6 : 3);
	await expect(top.first()).toContainText('Lena Probe');
	await expect(top.first()).toContainText('28.19');
	await keep('91-everyone-front');

	// The head stands over the plain list only: another order has its own.
	await page.getByRole('radio', { name: 'Best' }).check();
	await expect(page.getByRole('list', { name: 'Best of today' })).toHaveCount(0);
	await page.getByRole('radio', { name: 'Newest' }).check();
	await page.getByRole('link', { name: 'The whole ranking' }).click();
	await expect(page).toHaveURL(/\/logbook\/ranking\/$/);
	expect(problems).toEqual([]);
});
