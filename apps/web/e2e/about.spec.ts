/**
 * What Venti stands for, and its news: pages of the web's own, built from
 * the repository, which need no answer of the server.
 */

import { expect, test } from './helpers';

test('the about page says what Venti stands for, with the code and the developers’ pages', async ({
	page,
	keep,
	problems
}) => {
	await page.goto('/');
	await page
		.getByRole('region', { name: 'What Venti stands for' })
		.getByRole('link', { name: 'What Venti stands for' })
		.click();
	await expect(page).toHaveTitle('About · Venti');
	await expect(
		page.getByRole('heading', { level: 1, name: 'What Venti stands for' })
	).toBeVisible();
	for (const name of [
		'Free and open source',
		'No advertising, no tracking',
		'The data is the community’s',
		'Made by pilots'
	]) {
		await expect(page.getByRole('heading', { level: 2, name })).toBeVisible();
	}
	await expect(page.getByRole('link', { name: 'The source code' })).toHaveAttribute(
		'href',
		'https://github.com/dariobanfi/venti'
	);
	await expect(page.getByRole('link', { name: 'For developers' })).toHaveAttribute(
		'href',
		/\/developers\/?$/
	);
	await expect(page.getByText(/donat/i)).toHaveCount(0);
	await keep('204-about');
	expect(problems).toEqual([]);
});

test('a news post opens from the home page, with its picture, as a draft', async ({
	page,
	keep,
	problems
}) => {
	await page.goto('/');
	await page
		.getByRole('region', { name: 'News' })
		.getByRole('link', { name: 'Welcome to Venti' })
		.click();
	await expect(page).toHaveTitle('Welcome to Venti · Venti');
	await expect(page.getByRole('heading', { level: 1, name: 'Welcome to Venti' })).toBeVisible();
	await expect(page.getByText('Draft', { exact: true })).toBeVisible();
	await expect(page.getByRole('img', { name: /paragliders over an Alpine valley/ })).toBeVisible();
	await expect(
		page.getByRole('heading', { level: 2, name: 'What we want it to be' })
	).toBeVisible();
	await keep('205-news-post');

	// All the news, the newest first.
	await page.getByRole('link', { name: 'News', exact: true }).click();
	await expect(page).toHaveTitle('News · Venti');
	await expect(page.getByRole('link', { name: 'Welcome to Venti' })).toBeVisible();
	await keep('206-news');
	expect(problems).toEqual([]);
});
