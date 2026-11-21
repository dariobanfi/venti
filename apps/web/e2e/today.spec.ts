/**
 * Today, from Venti in its development mode: the take-offs OpenStreetMap
 * had around the middle of the first region, and the recorded forecast of
 * a stormy day in November. Its clock stands still in the evening of
 * 18 November 2026, so today has no flying hours left and the page opens
 * on tomorrow.
 */

import { expect, test } from './helpers';
import { madeUpSummer } from './made-up-summer';

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
	await expect(page.getByText(/They\s+are not\s+advice to fly/)).toBeVisible();
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

test('a strong day puts the take-offs in the order of their thermals, with the climb hour by hour', async ({
	context,
	page,
	keep,
	problems
}) => {
	// MADE UP: no model forecast this. See `made-up-summer.ts`.
	await context.route('**/v1/forecast**', madeUpSummer);
	await page.goto('/today/');

	// The head names the best take-off's kind of day under its line.
	await expect(page.getByText(/^Big day$/).first()).toBeVisible({ timeout: 30_000 });
	const best = page.locator('.best');
	// The card: the kind of day beside its mark, and the core's line under it.
	await expect(best.getByText('Big day', { exact: true })).toBeVisible();
	await expect(
		best.getByText(/^\d\.\d m\/s to a cloud base of \d+ m, from (\d+:00|noon) to \d+:00$/)
	).toBeVisible();
	// The hours carry the climb to expect, and the caption says what it is.
	const hours = page.getByRole('list', { name: 'Hour by hour' }).getByRole('listitem');
	await expect(hours.first()).toContainText(/\d\.\d/);
	await expect(hours.filter({ hasText: /thermals \d\.\d m\/s to \d+ m/ }).first()).toBeAttached();
	await expect(best).toContainText('the climb to expect in thermals, in m/s');

	// The others: a stronger day before a weaker one, each with its kind.
	const others = page.getByRole('list', { name: 'Other take-offs' }).getByRole('listitem');
	await expect(others.first()).toContainText(/Big day|Cross-country day|Local thermals/);
	const kinds = await others.locator('.kind-of-day').allTextContents();
	const order = ['Big day', 'Cross-country day', 'Local thermals', 'Weak thermals'];
	const ranks = kinds.map((kind) => order.indexOf(kind.trim()));
	expect(ranks).toEqual(ranks.toSorted((one, other) => one - other));

	// What the kind of day is, and the core's sentence about the estimate.
	await expect(
		page.getByText(/what the model’s heat and temperatures say of thermals/)
	).toBeVisible();
	await expect(page.getByText(/Thermals are an estimate from the model.s heat/)).toBeVisible();
	await expect(page.getByText(/\bsafe\b|\bgo\b/i)).toHaveCount(0);
	await keep('105-today-thermals');
	expect(problems).toEqual([]);
});
