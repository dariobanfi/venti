/**
 * Bringing flights into the logbook: by choosing files and by dropping them
 * on the page, several at once, with files among them that are not flights
 * or are there already.
 */

import {
	dropFiles,
	expect,
	mapTabDrawn,
	metric,
	SAMPLE_FLIGHT,
	sampleFlightOn,
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
	files.map((file) => ({
		name: file.name,
		mimeType: 'application/octet-stream',
		buffer: Buffer.from(file.text)
	}));

test('flights are imported with the file picker, several at once', async ({ page, keep, wide }) => {
	const totals = page.getByLabel('All flights');
	await page.goto('/logbook/');
	await expect(page.getByRole('heading', { name: 'No flights yet' })).toBeVisible();

	// The button opens the browser's file picker. Any file can be chosen.
	const picker = page.waitForEvent('filechooser');
	await page.getByRole('button', { name: 'Import flights' }).first().click();
	const chooser = await picker;
	expect(chooser.isMultiple()).toBe(true);
	await chooser.setFiles(asUploads(mixed));

	// The file that is not a flight is named, with the core's reason.
	const notice = page.getByRole('dialog', { name: '1 file could not be imported' });
	await expect(notice).toBeVisible();
	await expect(notice).toContainText(
		'notes.txt: This is not a flight file: it has no date. Flight files end in .igc.'
	);
	await keep('13-import-notice');
	await notice.getByRole('button', { name: 'OK' }).click();
	await expect(notice).toBeHidden();

	// The three flights are listed newest first, with their totals.
	// A row of the list on a phone, of the table on a wide screen.
	const rows = page
		.getByRole('main')
		.getByRole('link', { name: /(Mon|Tue|Wed|Thu|Fri|Sat|Sun), \d+ / });
	await expect(rows).toHaveCount(3);
	await expect(rows.nth(0)).toContainText('Sun, 5 Jul');
	await expect(rows.nth(1)).toContainText('Fri, 3 Jul');
	await expect(rows.nth(2)).toContainText('Thu, 2 Jul');
	await expect(metric(totals, 'Flights')).toHaveText('3');
	await expect(metric(totals, 'Airtime')).toHaveText('1:27 h');
	await expect(metric(totals, 'Distance')).toHaveText('36 km');
	if (!wide) await expect(page.getByText('3 flights · 1:27 h')).toBeVisible();

	// A flight that is already there is recognised, whatever its file is called.
	await page
		.locator('input[type=file]')
		.setInputFiles(asUploads([{ ...mixed[0], name: 'copy.igc' }]));
	const known = page.getByRole('dialog', { name: 'Already in your logbook' });
	await expect(known).toContainText('This flight was imported before. Nothing was added.');
	await known.getByRole('button', { name: 'OK' }).click();
	await expect(rows).toHaveCount(3);

	// They are stored in the browser: all three are there after a reload.
	await page.reload();
	await expect(rows).toHaveCount(3);
	await keep('14-logbook-several');
});

test('flights are imported by dropping files on the page', async ({ page, keep, mapDrawn }) => {
	// On the map, with nothing in the logbook yet.
	await page.goto('/');
	await expect(page.getByRole('button', { name: 'Show my position' })).toBeVisible();
	await mapTabDrawn(page, mapDrawn);

	// While files hover over the page, it says what dropping them does.
	const transfer = await page.evaluateHandle(() => {
		const transfer = new DataTransfer();
		transfer.items.add(new File(['x'], 'flight.igc'));
		return transfer;
	});
	await page.dispatchEvent('body', 'dragenter', { dataTransfer: transfer });
	await expect(page.getByText('Drop flight files to import them')).toBeVisible();
	await keep('15-drop');
	await page.dispatchEvent('body', 'dragleave', { dataTransfer: transfer });
	await expect(page.getByText('Drop flight files to import them')).toBeHidden();

	// Several flights dropped at once stay on the screen they were dropped on.
	await dropFiles(page, [mixed[0], mixed[1]]);
	await page.getByRole('link', { name: 'Logbook', exact: true }).click();
	await expect(
		page.getByRole('main').getByRole('link', { name: /(Mon|Tue|Wed|Thu|Fri|Sat|Sun), \d+ / })
	).toHaveCount(2);

	// A single new flight opens straight away, as it does from the picker.
	await dropFiles(page, [mixed[3]]);
	await expect(page.getByRole('heading', { name: 'Sun, 5 Jul 2026' })).toBeVisible();

	// Only a file that is not a flight: nothing is added, and the page says why.
	await dropFiles(page, [mixed[2]]);
	const notice = page.getByRole('dialog', { name: '1 file could not be imported' });
	await expect(notice).toContainText('notes.txt: This is not a flight file');
	await notice.getByRole('button', { name: 'OK' }).click();
});

test('the wide logbook is a table to sort, search and choose several flights in', async ({
	page,
	wide,
	keep,
	mapDrawn
}) => {
	test.skip(!wide, 'A narrow screen lists the flights, and deletes from the flight’s own screen.');

	await page.goto('/logbook/');
	await expect(page.getByRole('heading', { name: 'No flights yet' })).toBeVisible();
	await page
		.locator('input[type=file]')
		.setInputFiles(asUploads(mixed.filter((file) => file.name.endsWith('.igc'))));
	await page.getByRole('link', { name: 'Logbook', exact: true }).first().click();
	const table = page.getByRole('table', { name: 'Flights' });
	const rows = table.getByRole('row');
	// A heading and three flights, newest first, with room for more figures.
	await expect(rows).toHaveCount(4);
	await expect(rows.nth(1)).toContainText('Sun, 5 Jul');
	await expect(rows.nth(1)).toContainText('2117 m');
	await expect(rows.nth(1)).toContainText('+2.4 m/s');

	// A heading sorts, and a second click turns the order round.
	const date = table.getByRole('columnheader', { name: 'Date' });
	await expect(date).toHaveAttribute('aria-sort', 'descending');
	await date.getByRole('button').click();
	await expect(date).toHaveAttribute('aria-sort', 'ascending');
	await expect(rows.nth(1)).toContainText('Thu, 2 Jul');

	// A search that finds nothing says so.
	await page.getByRole('searchbox', { name: 'Search flights' }).fill('Nowhere');
	await expect(table).toContainText('No flight answers this search.');
	await page.getByRole('searchbox', { name: 'Search flights' }).fill('');

	// Two chosen: downloading and deleting them at once, deleting asked first.
	await page.getByRole('checkbox', { name: 'Choose the flight of Thu, 2 Jul 2026' }).check();
	await page.getByRole('checkbox', { name: 'Choose the flight of Fri, 3 Jul 2026' }).check();
	await expect(page.getByText('2 of 3 flights chosen')).toBeVisible();
	await expect(page.getByRole('button', { name: 'Download 2 files' })).toBeVisible();
	await keep('110-logbook-table');
	await page.getByRole('button', { name: 'Delete', exact: true }).click();
	const confirmation = page.getByRole('alertdialog', { name: 'Delete 2 flights?' });
	await expect(confirmation).toContainText('Their files are removed');
	await confirmation.getByRole('button', { name: 'Cancel' }).click();
	await expect(rows).toHaveCount(4);
	await page.getByRole('button', { name: 'Delete', exact: true }).click();
	await confirmation.getByRole('button', { name: 'Delete 2 flights' }).click();
	await expect(rows).toHaveCount(2);

	// A row opens its flight; on a large screen the logbook stays beside it.
	await rows.nth(1).getByRole('cell').nth(2).click();
	await expect(page.getByRole('heading', { name: 'Sun, 5 Jul 2026' })).toBeVisible();
	const beside = page.getByRole('complementary', { name: 'Logbook' });
	if ((page.viewportSize()?.width ?? 0) >= 1280) {
		await expect(beside.getByRole('link', { name: /5 Jul/ })).toHaveAttribute(
			'aria-current',
			'page'
		);
		await mapDrawn();
		await keep('111-flight-beside-logbook');
	} else {
		await expect(beside).toHaveCount(0);
	}
});

test('two tabs share one logbook', async ({ context, page }) => {
	await page.goto('/logbook/');
	await expect(page.getByRole('heading', { name: 'No flights yet' })).toBeVisible();

	// A second tab of the same browser imports a flight.
	const second = await context.newPage();
	await second.goto('/logbook/');
	await second.getByRole('button', { name: 'Add a sample flight' }).click();
	await expect(second.getByRole('heading', { name: 'Sat, 4 Jul 2026' })).toBeVisible();

	// The first tab shows it without being reloaded.
	const row = page.getByRole('link', { name: /Sat, 4 Jul/ });
	await expect(row).toBeVisible();

	// And when the first tab deletes it, the second one's logbook is empty.
	await row.click();
	await page.getByRole('button', { name: 'More' }).click();
	await page.getByRole('menuitem', { name: 'Delete flight' }).click();
	await page.getByRole('button', { name: 'Delete flight' }).click();
	await expect(page.getByRole('heading', { name: 'No flights yet' })).toBeVisible();
	await second.getByRole('link', { name: 'Logbook', exact: true }).first().click();
	await expect(second.getByRole('heading', { name: 'No flights yet' })).toBeVisible();
});
