/**
 * Today, from Venti in its development mode: the take-offs OpenStreetMap
 * had around the middle of the first region, and the recorded forecast of
 * a stormy day in November. Its clock stands still in the evening of
 * 18 November 2026, so today has no flying hours left and the page opens
 * on tomorrow.
 */

import { expect, test } from './helpers';

/** Where OpenStreetMap has the take-off of the Jägerkamp. */
const JAEGERKAMP = { latitude: 47.6791413, longitude: 11.9051032 };

test('today says what the forecast says of the take-offs around', async ({
	page,
	keep,
	problems
}) => {
	await page.goto('/today/');
	await expect(page).toHaveTitle('Today · Venti');
	await expect(
		page.getByRole('navigation', { name: 'Sections' }).getByRole('link', { name: 'Today' })
	).toHaveAttribute('aria-current', 'page');

	// The take-offs are listed at once, and each forecast fills in.
	const others = page.getByRole('list', { name: 'Other take-offs' }).getByRole('listitem');
	await expect(others.first()).toBeVisible();
	await expect(page.getByText(/^(Flyable|Marginal|Not flyable)$/).first()).toBeVisible({
		timeout: 30_000
	});
	// A stormy day: the gusts are what speaks against it.
	await expect(page.getByText('Gusts', { exact: true })).toBeVisible();
	await expect(page.getByRole('list', { name: 'Hour by hour' })).toBeVisible();
	// Never a word that sounds like permission.
	await expect(page.getByText(/\bsafe\b/i)).toHaveCount(0);
	await expect(page.getByText(/They are not\s+advice to fly/)).toBeVisible();
	await expect(page.getByText(/run of/)).toBeVisible();
	await keep('96-today');

	// Another day.
	const days = page.getByRole('radiogroup', { name: 'Day' }).getByRole('radio');
	await expect(days.first()).toBeVisible();
	await days.last().check();
	await expect(days.last()).toBeChecked();

	// A take-off leads to its page.
	await others.first().getByRole('link').click();
	await expect(page).toHaveURL(/\/logbook\/everyone\/place\/\?lat=/);
	expect(problems).toEqual([]);
});

test('today goes by the pilot’s position once asked to', async ({ context, page, keep }) => {
	await context.grantPermissions(['geolocation']);
	await context.setGeolocation(JAEGERKAMP);
	await page.goto('/today/');
	await expect(page.getByText(/around the Tegernsee$/)).toBeVisible();
	await page.getByRole('button', { name: 'Use my position' }).click();
	await expect(page.getByText(/near you$/)).toBeVisible();
	await expect(page.getByRole('button', { name: 'Use my position' })).toHaveCount(0);
	await expect(page.getByText(/^(Flyable|Marginal|Not flyable)$/).first()).toBeVisible({
		timeout: 30_000
	});
	await keep('97-today-near-you');
});

test('today says when Venti cannot be reached, and asks again', async ({ context, page, keep }) => {
	let down = true;
	await context.route('**/v1/sites**', (route) =>
		down ? route.fulfill({ status: 503, body: '' }) : route.continue()
	);
	await page.goto('/today/');
	await expect(
		page.getByRole('heading', { name: 'Today’s forecast could not be fetched' })
	).toBeVisible();
	await keep('98-today-unreachable');
	down = false;
	await page.getByRole('button', { name: 'Try again' }).click();
	await expect(page.getByRole('list', { name: 'Other take-offs' })).toBeVisible();
});
