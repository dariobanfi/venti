/**
 * Today, from Venti in its development mode: the take-offs OpenStreetMap
 * had around the middle of the first region, and the recorded forecast of
 * a stormy day in November. Its clock stands still in the evening of
 * 18 November 2026, so today has no flying hours left and the page opens
 * on tomorrow.
 */

import { expect, rowsOf, test } from './helpers';
import { madeUpSummer } from './made-up-summer';

/** Where OpenStreetMap has the take-off of the Jägerkamp. */
const JAEGERKAMP = { latitude: 47.6791413, longitude: 11.9051032 };

test('today says what the forecast says of the take-offs around', async ({
	page,
	keep,
	wide,
	problems
}) => {
	await page.goto('/');
	// Today is the home page's first part (the owner, 2026-11-23), and the
	// bar has no section of its own for it.
	await expect(page).toHaveTitle('Venti');
	const sections = page.getByRole('navigation', { name: 'Sections' });
	await expect(sections.getByRole('link', { name: 'Home', exact: true })).toHaveAttribute(
		'aria-current',
		'page'
	);
	await expect(sections.getByRole('link', { name: 'Today' })).toHaveCount(0);
	const today = page.getByRole('region', { name: 'Today', exact: true });
	const standsFor = page.getByRole('region', { name: 'What Venti stands for' });
	expect((await today.boundingBox())!.y).toBeLessThan((await standsFor.boundingBox())!.y);

	// The take-offs are listed at once, and each forecast fills in.
	// A wide screen has every take-off in a table; a phone, the others under the best.
	const others = wide
		? rowsOf(page, 'Take-offs')
		: page.getByRole('list', { name: 'Other take-offs' }).getByRole('listitem');
	await expect(others.first()).toBeVisible();
	await expect(page.getByText(/^(Flyable|Marginal|Not flyable)$/).first()).toBeVisible({
		timeout: 30_000
	});
	// A stormy day: the gusts are what speaks against it.
	await expect(page.getByText('Gusts', { exact: true })).toBeVisible();
	await expect(page.getByRole('list', { name: 'Hour by hour' })).toBeVisible();
	// Never a word that sounds like permission.
	await expect(today.getByText(/\bsafe\b/i)).toHaveCount(0);
	await expect(page.getByText(/They\s+are not\s+advice to fly/)).toBeVisible();
	// The model's run is one click away, behind "About this forecast".
	await expect(page.getByText(/run of/)).toBeHidden();
	await keep('96-today');
	await page.getByText('About this forecast').click();
	await expect(page.getByText(/run of/)).toBeVisible();

	// Another day.
	const days = page.getByRole('radiogroup', { name: 'Day' }).getByRole('radio');
	await expect(days.first()).toBeVisible();
	await days.last().check();
	await expect(days.last()).toBeChecked();

	// A take-off leads to its page. On a wide screen a row chosen fills
	// the card beside the table, whose title leads there.
	if (wide) {
		const second = others.nth(1).getByRole('button');
		const name = (await second.textContent())!.trim();
		await second.click();
		await expect(second).toHaveAttribute('aria-pressed', 'true');
		await page.locator('.best h2').getByRole('link', { name }).click();
	} else {
		await others.first().getByRole('link').click();
	}
	await expect(page).toHaveURL(/\/places\/[-0-9.]+,[-0-9.]+\//);
	expect(problems).toEqual([]);
});

test('today goes by the pilot’s position once asked to', async ({ context, page, keep }) => {
	await context.grantPermissions(['geolocation']);
	await context.setGeolocation(JAEGERKAMP);
	await page.goto('/');
	// A position the browser was allowed to give before is used without asking.
	await expect(page.getByText(/near you$/)).toBeVisible();
	await expect(page.getByRole('button', { name: 'Use my position' })).toHaveCount(0);
	await context.clearPermissions();
	await page.reload();
	// Else the place is the region's middle, or the position of the visit before.
	await expect(page.getByText(/(around the Tegernsee|near your last position)$/)).toBeVisible();
	await context.grantPermissions(['geolocation']);
	await page.getByRole('button', { name: 'Use my position' }).click();
	await expect(page.getByText(/near you$/)).toBeVisible();
	await expect(page.getByRole('button', { name: 'Use my position' })).toHaveCount(0);
	await expect(page.getByText(/^(Flyable|Marginal|Not flyable)$/).first()).toBeVisible({
		timeout: 30_000
	});
	await keep('97-today-near-you');
});

test('today says when Venti cannot be reached, and asks again', async ({
	context,
	page,
	keep,
	wide
}) => {
	let down = true;
	await context.route('**/v1/sites**', (route) =>
		down ? route.fulfill({ status: 503, body: '' }) : route.continue()
	);
	await page.goto('/');
	await expect(
		page.getByRole('heading', { name: 'Today’s forecast could not be fetched' })
	).toBeVisible();
	await keep('98-today-unreachable');
	down = false;
	await page
		.getByRole('region', { name: 'Today', exact: true })
		.getByRole('button', { name: 'Try again' })
		.click();
	await expect(
		wide
			? page.getByRole('table', { name: 'Take-offs' })
			: page.getByRole('list', { name: 'Other take-offs' })
	).toBeVisible();
});

test('a strong day puts the take-offs in the order of their thermals, with the climb hour by hour', async ({
	context,
	page,
	keep,
	wide,
	problems
}) => {
	// MADE UP: no model forecast this. See `made-up-summer.ts`.
	await context.route('**/v1/forecast**', madeUpSummer);
	await page.goto('/');

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
	// What the strip shows, in the core's words.
	await expect(best).toContainText('Wind in km/h at 10 m, the climb in m/s.');

	// The others: a stronger day before a weaker one, each with its kind.
	const others = wide
		? rowsOf(page, 'Take-offs')
		: page.getByRole('list', { name: 'Other take-offs' }).getByRole('listitem');
	await expect(others.first()).toContainText(/Big day|Cross-country day|Local thermals/);
	const kinds = await others.locator('.kind-of-day').allTextContents();
	const order = ['Big day', 'Cross-country day', 'Local thermals', 'Weak thermals'];
	const ranks = kinds.map((kind) => order.indexOf(kind.trim()));
	expect(ranks).toEqual(ranks.toSorted((one, other) => one - other));

	// What the kind of day is, and the core's sentence about the estimate.
	await expect(
		page.getByText(/what the model’s heat and temperatures say of thermals/)
	).toBeVisible();
	// With the model's grid cell and its run, behind "About this forecast".
	await page.getByText('About this forecast').click();
	await expect(page.getByText(/Thermals are an estimate from the model.s heat/)).toBeVisible();
	await expect(page.getByText(/\bsafe\b|\bgo\b/i)).toHaveCount(0);
	await keep('105-today-thermals');
	expect(problems).toEqual([]);
});

test('a take-off the pilot keeps leads Today under its heading, and is forgotten on its page', async ({
	page,
	keep,
	wide,
	problems
}) => {
	const NAME = 'Fluggelände Jägerkamp/Nagelspitz';
	const sections = page.getByRole('navigation', { name: 'Sections' });
	const forecasts = () =>
		expect(page.getByText(/^(Flyable|Marginal|Not flyable)$/).first()).toBeVisible({
			timeout: 30_000
		});

	// With none kept, Today has one group and names none.
	await page.goto('/');
	await forecasts();
	await expect(page.getByText('Your take-offs')).toHaveCount(0);
	await expect(page.getByText('Within 30 km')).toHaveCount(0);

	// The take-off's page, as Today leads to it, has the row that keeps it.
	if (wide) {
		await rowsOf(page, 'Take-offs').filter({ hasText: NAME }).getByRole('button').click();
		await page.locator('.best h2').getByRole('link', { name: NAME }).click();
	} else {
		await page
			.getByRole('link', { name: new RegExp(NAME) })
			.first()
			.click();
	}
	await expect(page).toHaveURL(/\/places\/[-0-9.]+,[-0-9.]+\//);
	const sitePage = page.url();
	await page.getByRole('button', { name: 'Keep as my take-off' }).click();
	await expect(page.getByRole('button', { name: 'Kept' })).toHaveAttribute('aria-pressed', 'true');
	// What its source says of a fee, and its web page.
	const more = page.getByRole('region', { name: 'What its source says' });
	await expect(more).toContainText('A fee is asked for using this site.');
	await expect(more.getByRole('link', { name: /^Web page/ })).toHaveAttribute(
		'href',
		'https://www.dhv.de/'
	);
	await keep('132-site-kept');

	// Today, drawn again, has it first under the core's heading, and the
	// others under theirs. The same after a reload: the browser kept it.
	await sections.getByRole('link', { name: 'Home', exact: true }).click();
	for (const reloaded of [false, true]) {
		if (reloaded) await page.reload();
		await forecasts();
		if (wide) {
			const rows = rowsOf(page, 'Take-offs');
			await expect(rows.nth(0)).toHaveText('Your take-offs');
			await expect(rows.nth(1)).toContainText(NAME);
			await expect(rows.nth(2)).toHaveText('Within 30 km');
		} else {
			// The best of the day stands apart as the card, and says which group it is of.
			const first = page
				.locator('.best')
				.or(page.getByRole('list', { name: 'Your take-offs' }))
				.first();
			await expect(first).toContainText(NAME);
			await expect(page.getByText('Your take-offs')).toBeVisible();
			await expect(page.getByRole('heading', { name: 'Within 30 km' })).toBeVisible();
			await expect(
				page.getByRole('list', { name: 'Within 30 km' }).getByRole('listitem').first()
			).toBeVisible();
			await expect(page.getByRole('list', { name: 'Within 30 km' })).not.toContainText(NAME);
		}
	}
	// Whose the take-offs are.
	await expect(page.getByRole('link', { name: '© OpenStreetMap contributors' })).toBeVisible();
	await keep('133-today-kept');

	// Forgotten on its page, it no longer leads Today.
	await page.goto(sitePage);
	await page.getByRole('button', { name: 'Kept' }).click();
	await expect(page.getByRole('button', { name: 'Keep as my take-off' })).toHaveAttribute(
		'aria-pressed',
		'false'
	);
	await sections.getByRole('link', { name: 'Home', exact: true }).click();
	await forecasts();
	await expect(page.getByText('Your take-offs')).toHaveCount(0);
	await expect(page.getByText('Within 30 km')).toHaveCount(0);
	expect(problems).toEqual([]);
});

test('a day on which the wind near by is much stronger says so under what limits the day', async ({
	page,
	keep,
	wide,
	problems
}) => {
	await page.goto('/');
	await expect(page.getByText(/^(Flyable|Marginal|Not flyable)$/).first()).toBeVisible({
		timeout: 30_000
	});
	// The recorded forecast has it on its Monday, at one take-off at least.
	const days = page.getByRole('radiogroup', { name: 'Day' });
	await days.getByText('Mon', { exact: true }).click();
	await expect(days.getByRole('radio', { name: 'Mon' })).toBeChecked();
	const line = page.getByTestId('nearby-line').first();
	// In a list of take-offs the core's short form, in the card of the one
	// chosen its whole line.
	await expect(line).toHaveText(
		/^(Much stronger near by at \d+:00: .+ \d+ km\/h .+ \d+ km\/h at the place|(Gusts to|Wind) \d+ km\/h near by at \d+:00)$/,
		{
			timeout: 30_000
		}
	);
	if (wide) {
		// In the table it stands under the status and its reason; the row
		// chosen has it in the card beside the table too.
		const row = rowsOf(page, 'Take-offs')
			.filter({ has: page.getByTestId('nearby-line') })
			.first();
		await expect(row.locator('td.status span.status')).toHaveText(
			/^\s*(Flyable|Marginal|Not flyable)\s*$/
		);
		await row.getByRole('button').click();
		await expect(page.locator('.best').getByTestId('nearby-line')).toBeVisible();
	}
	// It is a note, not a status: the day keeps the one status it had.
	await expect(page.locator('.best .status')).toHaveText(/^\s*(Flyable|Marginal|Not flyable)\s*$/);
	// What "near by" is and is not, at the foot.
	await expect(page.getByText(/^Near by is the strongest wind and gust/)).toBeVisible();
	await line.scrollIntoViewIfNeeded();
	await keep('150-today-nearby');
	expect(problems).toEqual([]);
});
