/**
 * The pilot's flights on the website: on the account only (owner decision
 * 20). A visitor who is not signed in is asked to; files brought in by the
 * picker or by dropping them on the page are uploads to the account, several
 * at once, with files among them that are not flights or are there already;
 * the list is by day; and My flights is reached from the account and from
 * Flights, and from the logbook's old address. The statistics of all the
 * pilot's flights, the private ones too, are the server's, read by the core.
 */

import { readFileSync } from 'node:fs';
import {
	addSampleFlight,
	asUpload,
	dropFiles,
	expect,
	mapTabDrawn,
	SAMPLE_FLIGHT,
	sampleFlightOn,
	signUp,
	test
} from './helpers';

/** The files a pilot might select together: three flights of July 2026 and a text file. */
const mixed = [
	{ name: '2026-07-02.igc', text: sampleFlightOn(2) },
	{ name: '2026-07-03.igc', text: sampleFlightOn(3) },
	{ name: 'notes.txt', text: 'Wallberg, north wind, cold.' },
	{ name: '2026-07-05.igc', text: sampleFlightOn(5) }
];

const asUploads = (files: { name: string; text: string }[]) =>
	files.map((file) => asUpload(file.name, file.text));

/** The rows of the list, by day: a list of each day's flights, named by the day. */
const flightRows = (page: import('@playwright/test').Page) =>
	page.getByRole('main').locator('section.day li');

test('a visitor who is not signed in is asked to, and the logbook’s address leads here', async ({
	page,
	keep,
	problems
}) => {
	// The logbook's old address, as others link to it.
	await page.goto('/logbook/');
	await expect(page).toHaveURL(/\/settings\/account\/flights\/$/);
	await expect(page).toHaveTitle('My flights · Venti');
	await expect(page.getByRole('heading', { name: 'Sign in to see your flights' })).toBeVisible();
	await expect(
		page.getByRole('main').getByRole('link', { name: 'Sign in', exact: true })
	).toHaveAttribute('href', '/settings/account');
	await keep('13-my-flights-signed-out');

	// A file dropped without an account is not kept anywhere: the page says to sign in.
	await dropFiles(page, [mixed[0]]);
	const notice = page.getByRole('dialog', { name: 'Sign in to bring in flights' });
	await expect(notice).toBeVisible();
	await notice.getByRole('button', { name: 'OK' }).click();

	// An old address of a flight leads to its address on the account.
	await page.goto('/logbook/flight/?id=5t1eshypdtvrq7rxw39a18vsrg');
	await expect(page).toHaveURL(
		/\/settings\/account\/flights\/flight\/\?id=5t1eshypdtvrq7rxw39a18vsrg$/
	);
	await expect(page.getByRole('heading', { name: 'Sign in to see this flight' })).toBeVisible();

	expect(problems).toEqual([]);
});

test('flights are brought in with the file picker, several at once, and listed by day', async ({
	page,
	keep,
	problems
}) => {
	await signUp(page);
	// The account leads to the pilot's flights.
	await page.getByRole('link', { name: 'My flights' }).click();
	await expect(page.getByRole('heading', { level: 1, name: 'My flights' })).toBeVisible();
	await expect(page.getByRole('heading', { name: 'No flights yet' })).toBeVisible();

	// The button opens the browser's file picker. Any file can be chosen.
	const picker = page.waitForEvent('filechooser');
	await page.getByRole('button', { name: 'Bring in flights' }).first().click();
	const chooser = await picker;
	expect(chooser.isMultiple()).toBe(true);
	await chooser.setFiles(asUploads(mixed));

	// The file that is not a flight is named, with the reason.
	const notice = page.getByRole('dialog', { name: '1 file could not be brought in' });
	await expect(notice).toBeVisible();
	await expect(notice).toContainText('notes.txt: ');
	await keep('13-import-notice');
	await notice.getByRole('button', { name: 'OK' }).click();
	await expect(notice).toBeHidden();

	// The three flights, a day each, the newest first: the take-off's time,
	// the glider of the file, the airtime large and the distance.
	const days = page.getByRole('main').locator('section.day h2');
	await expect(days).toHaveText([/^Sun, 5 Jul 2026/, /^Fri, 3 Jul 2026/, /^Thu, 2 Jul 2026/]);
	await expect(flightRows(page)).toHaveCount(3);
	const first = flightRows(page).first();
	await expect(first).toContainText('Sample Wing');
	await expect(first.locator('.time')).toHaveText(/\d\d:\d\d/);
	await expect(first.locator('.airtime')).toHaveText(/0:29\s*h/);
	await expect(first.locator('.distance')).toHaveText(/\d+\.\d km/);
	await expect(first.locator('svg polyline')).toHaveCount(1);
	await keep('14-my-flights');

	// A flight that is already there is recognised, whatever its file is called.
	await page
		.locator('input[type=file]')
		.setInputFiles(asUploads([{ ...mixed[0], name: 'copy.igc' }]));
	const known = page.getByRole('dialog', { name: 'Already on your account' });
	await expect(known).toContainText('This flight was brought in before. Nothing was added.');
	await known.getByRole('button', { name: 'OK' }).click();
	await expect(flightRows(page)).toHaveCount(3);

	// They are the account's: all three are there after a reload.
	await page.reload();
	await expect(flightRows(page)).toHaveCount(3);

	// A row opens its flight.
	await first.getByRole('link').click();
	await expect(page.getByRole('heading', { name: 'Sun, 5 Jul 2026' })).toBeVisible();

	expect(problems).toEqual([]);
});

test('flights are brought in by dropping files on the page, and found from Flights', async ({
	page,
	keep,
	mapDrawn,
	wide,
	problems
}) => {
	await signUp(page);
	// On the map, with no flight on the account yet.
	await page.goto('/map/');
	await expect(page.getByRole('button', { name: 'Show my position' })).toBeVisible();
	await mapTabDrawn(page, mapDrawn);

	// While files hover over the page, it says what dropping them does.
	const transfer = await page.evaluateHandle(() => {
		const transfer = new DataTransfer();
		transfer.items.add(new File(['x'], 'flight.igc'));
		return transfer;
	});
	await page.dispatchEvent('body', 'dragenter', { dataTransfer: transfer });
	await expect(page.getByText('Drop flight files to bring them in')).toBeVisible();
	await keep('15-drop');
	await page.dispatchEvent('body', 'dragleave', { dataTransfer: transfer });
	await expect(page.getByText('Drop flight files to bring them in')).toBeHidden();

	// Several flights dropped at once stay on the screen they were dropped on.
	await dropFiles(page, [mixed[0], mixed[1]]);

	// The navigation has no logbook: the pilot's flights are under Flights, as "Mine".
	const sections = page.getByRole('navigation', { name: 'Sections' });
	await expect(sections.getByRole('link', { name: 'Logbook' })).toHaveCount(0);
	await sections.getByRole('link', { name: 'Flights', exact: true }).click();
	await page
		.getByRole('navigation', { name: 'Whose flights' })
		.getByRole('link', { name: 'Mine' })
		.click();
	await expect(page.getByRole('heading', { level: 1, name: 'My flights' })).toBeVisible();
	await expect(flightRows(page)).toHaveCount(2);
	await expect(sections.getByRole('link', { name: 'Flights', exact: true })).toHaveAttribute(
		'aria-current',
		'page'
	);
	if (wide) await keep('16-my-flights-from-flights');

	// A single new flight opens straight away, as it does from the picker.
	await dropFiles(page, [mixed[3]]);
	await expect(page.getByRole('heading', { name: 'Sun, 5 Jul 2026' })).toBeVisible();

	// Only a file that is not a flight: nothing is added, and the page says why.
	await dropFiles(page, [mixed[2]]);
	const notice = page.getByRole('dialog', { name: '1 file could not be brought in' });
	await expect(notice).toContainText('notes.txt: ');
	await notice.getByRole('button', { name: 'OK' }).click();

	expect(problems).toEqual([]);
});

test('the statistics add up all the pilot’s flights, the private ones too', async ({
	page,
	keep,
	problems
}) => {
	await addSampleFlight(page);
	// Only the pilot sees it now; it still counts.
	await page.getByRole('button', { name: 'More' }).click();
	await page.getByRole('menuitem', { name: 'Share with a link' }).click();
	const sheet = page.getByRole('dialog', { name: 'Share flight' });
	await sheet.getByRole('radio', { name: 'Only me' }).check();
	await expect(sheet.getByRole('radio', { name: 'Only me' })).toBeChecked();
	await sheet.getByRole('button', { name: 'Done' }).click();

	await page.goto('/settings/account/flights/');
	await page.getByRole('main').getByRole('link', { name: 'Statistics', exact: true }).click();
	await expect(page.getByRole('heading', { name: 'Statistics', level: 1 })).toBeVisible();
	const totals = page.getByRole('region', { name: 'Totals' });
	const flights = totals.locator('dl .row').filter({ hasText: /^Flights/ });
	await expect(flights.locator('dd')).toHaveText('1');
	await expect(totals.locator('dl .row').filter({ hasText: /^Airtime/ })).toBeVisible();
	// The core's note under them is the pilot's own.
	await expect(
		totals.getByText(/^Of all your flights on the server, the private ones too/)
	).toBeVisible();
	await keep('09-my-statistics');

	// The logbook's old address of its statistics leads here.
	await page.goto('/logbook/statistics/');
	await expect(page).toHaveURL(/\/settings\/account\/flights\/statistics\/$/);
	await expect(flights.locator('dd')).toHaveText('1');
	expect(problems).toEqual([]);
});

test('how you fly is said from the flights on the account, each observation with the flight that shows it most', async ({
	page,
	keep,
	problems
}) => {
	// One flight has two thermals: too few to say anything.
	await addSampleFlight(page);
	await page.goto('/settings/account/flights/statistics/');
	await page.getByRole('link', { name: 'How you fly' }).click();
	await expect(page).toHaveTitle('How you fly · Venti');
	await expect(page.getByRole('heading', { level: 1, name: 'How you fly' })).toBeVisible();
	await expect(
		page.getByText(
			'Nothing to say yet. The observations begin once the logbook holds ten thermals.'
		)
	).toBeVisible();
	// What the observations are and are not, under them in any case.
	await expect(page.getByText(/not a judgement of you or of what is safe/)).toBeVisible();
	await keep('09b-how-you-fly-empty');

	// Five more copies on other days of July: ten thermals and more.
	await page.goto('/settings/account/flights/');
	await page
		.locator('input[type=file]')
		.setInputFiles(
			[1, 2, 3, 5, 6].map((day) => asUpload(`2026-07-0${day}.igc`, sampleFlightOn(day)))
		);
	await expect(page.getByRole('heading', { name: 'Mon, 6 Jul 2026' })).toBeVisible();
	// The logbook's old address leads here.
	await page.goto('/logbook/statistics/how-you-fly/');
	await expect(page).toHaveURL(/\/settings\/account\/flights\/statistics\/how-you-fly\/$/);
	const climbing = page.getByRole('group', { name: 'Climbing' });
	await expect(climbing).toContainText(/m\/s on average over \d+ m gained in thermals/);
	await expect(page.getByRole('group', { name: 'Which way you circle' })).toContainText(
		/thermals circled one way/
	);
	// Never a word of praise or of what is safe.
	await expect(page.getByRole('main').getByText(/\bshould\b|\bgood\b|\bsafe to\b/i)).toHaveCount(0);
	await keep('09c-how-you-fly');

	// The flight that shows it most opens that flight among the pilot's own.
	const flight = climbing.getByRole('link', { name: /The flight that shows it most/ });
	await expect(flight).toContainText(/\w{3}, \d+ Jul 2026/);
	await flight.click();
	await expect(page).toHaveURL(/\/settings\/account\/flights\/flight\/\?id=/);
	expect(problems).toEqual([]);
});

test.describe('in a browser in California', () => {
	test.use({ timezoneId: 'America/Los_Angeles' });

	test('a flight reads by the clock of the place it was flown, not the reader’s', async ({
		page,
		problems
	}) => {
		// The sample flight took off at 10:30 UTC at the Tegernsee: 12:30
		// there, 03:30 in Los Angeles.
		await signUp(page);
		await page.goto('/settings/account/flights/');
		await page
			.locator('input[type=file]')
			.setInputFiles(asUpload('sample.igc', readFileSync(SAMPLE_FLIGHT, 'utf8')));
		await expect(page.getByRole('heading', { name: 'Sat, 4 Jul 2026' })).toBeVisible();
		await expect(page.getByText('12:30 – 12:59 · Sample Wing')).toBeVisible();

		await page.goto('/settings/account/flights/');
		await expect(page.getByRole('main').locator('section.day h2')).toHaveText([/^Sat, 4 Jul 2026/]);
		await expect(flightRows(page).first().locator('.time')).toHaveText(/\b12:30$/);
		expect(problems).toEqual([]);
	});
});
