/**
 * Airspace on the maps: what the app fetches from the Venti server around
 * where the pilot looks and keeps in the browser, and the OpenAir files the
 * pilot brings in.
 *
 * The server is the development mode's: openAIP's airspace of the northern
 * Alps as recorded, among it the terminal area of Innsbruck, which reaches
 * over the valley of the sample flight.
 */

import type { Page } from '@playwright/test';
import { addSampleFlight, dropFiles, expect, IN_THE_VALLEY, mapTabDrawn, test } from './helpers';

/**
 * An OpenAir file as a pilot might bring it: a control zone of one nautical
 * mile over the valley of the sample flight, a danger area north-east of
 * it, and an airspace that cannot be read because it has no shape.
 */
const ZONES = `* Test airspace for Venti
AC D
AY CTR
AN CTR TEST
AL GND
AH 9000ft AMSL
V X=47:40:55 N 011:45:54 E
DC 1

AC Q
AN ED-D TEST
AL 1000ft AGL
AH FL 95
DP 48:00:00 N 012:00:00 E
DP 48:00:00 N 012:10:00 E
DP 48:05:00 N 012:10:00 E
DP 48:05:00 N 012:00:00 E

AC C
AN NO SHAPE
AL GND
AH FL 100
`;

const upload = (name: string, text: string) => ({
	name,
	mimeType: 'application/octet-stream',
	buffer: Buffer.from(text)
});

/** How many airspaces a map says it draws, for those who cannot see it. */
const drawn = (page: Page) => page.getByText(/^Airspace on this part of the map/);

const sections = (page: Page) => page.getByRole('navigation', { name: 'Sections' });

/** Opens the screen that lists the pilot's airspace, from wherever the navigation shows. */
async function openAirspace(page: Page): Promise<void> {
	await sections(page).getByRole('link', { name: 'Settings', exact: true }).click();
	await page.getByRole('link', { name: /^Airspace/ }).click();
	await expect(page.getByRole('heading', { level: 1, name: 'Airspace' })).toBeVisible();
}

/** Brings in files with the button on the airspace screen. */
async function addFiles(page: Page, files: ReturnType<typeof upload>[]): Promise<void> {
	const picker = page.waitForEvent('filechooser');
	await page.getByRole('button', { name: 'Add an airspace file' }).click();
	const chooser = await picker;
	expect(chooser.isMultiple()).toBe(true);
	await chooser.setFiles(files);
}

test('the map draws the airspace the server has for where the pilot looks, and says whose it is', async ({
	context,
	page,
	mapDrawn,
	keep,
	problems
}) => {
	await page.goto('/');
	await mapTabDrawn(page, mapDrawn);

	// Fetched around what the map shows, kept, and drawn: no file was needed.
	await expect(drawn(page)).toHaveText(/^Airspace on this part of the map: [1-9]\d*\.$/);
	// openAIP's licence asks for its credit and its notice wherever its
	// airspace is shown. On the map that is a line; the airspace screen has
	// it in full.
	await expect(page.getByText('Airspace: openAIP, not certified')).toBeVisible();
	await expect(page.getByText('Wind: Deutscher Wetterdienst and GeoSphere Austria')).toBeVisible();
	await mapDrawn();
	await keep('33-map-airspace');

	// Kept in the browser: opened again without the server's airspace, the
	// map still has it.
	await context.route('**/v1/airspace**', (route) => route.abort());
	await page.reload();
	await mapTabDrawn(page, mapDrawn);
	await expect(drawn(page)).toHaveText(/: [1-9]\d*\.$/);
	await expect(page.getByText('Airspace: openAIP, not certified')).toBeVisible();

	expect(problems).toEqual([]);
});

test('the airspace screen says what is kept and whose it is, takes files in and lets go of both', async ({
	page,
	mapDrawn,
	keep,
	wide
}) => {
	// The map fetches the server's airspace; the screen that lists it does not.
	await page.goto('/');
	await mapTabDrawn(page, mapDrawn);
	await expect(page.getByText('Airspace: openAIP, not certified')).toBeVisible();
	await sections(page).getByRole('link', { name: 'Settings', exact: true }).click();
	const row = page.getByRole('link', { name: /^Airspace/ });
	await expect(row).toHaveText(/openAIP, \d+ airspaces/);
	await row.click();
	await expect(page).toHaveTitle('Airspace · Venti');

	// What is kept from the server: how much, how old, of which countries,
	// and what its provider asks to be said about it.
	const fromServer = page.getByRole('list', { name: 'From this site’s server' });
	await expect(fromServer).toContainText('openAIP');
	await expect(fromServer).toContainText(/\d+ airspaces · as of \w{3}, \d+ \w{3} \d{4} · fetched /);
	await expect(fromServer).toContainText(
		'The airspace of Germany, Austria, Switzerland, Italy, Slovenia, Croatia, Hungary and Slovakia. Of other countries none is kept'
	);
	await expect(fromServer).toContainText(
		'OpenAIP data is not certified and must not be used for primary navigation or flight planning.'
	);
	await expect(fromServer).toContainText('Licence: CC BY-NC 4.0.');
	await expect(fromServer).toContainText('Data used comes from openAIP and is licensed under');
	await expect(fromServer.getByRole('link', { name: 'www.openaip.net' })).toHaveAttribute(
		'href',
		'https://www.openaip.net'
	);
	const fromTheServer = page.getByRole('switch', { name: 'Airspace from this site’s server' });
	await expect(fromTheServer).toBeChecked();
	await expect(page.getByText('Kept in this browser for about 200 km around')).toBeVisible();
	// The screen says what the app does not know.
	await expect(
		page.getByText('It knows the airspace listed here and no other.', { exact: false })
	).toBeVisible();
	await expect(page.getByText('What it draws is advice', { exact: false })).toBeVisible();

	// Files: the summary says what each gave, and above all what it left out.
	await addFiles(page, [
		upload('Test Zones.txt', ZONES),
		upload('notes.txt', 'Wallberg, north wind, cold.')
	]);
	const notice = page.getByRole('dialog', { name: 'Airspace' });
	await expect(notice).toContainText(
		'Test Zones.txt: 2 airspaces read. 1 airspace could not be read and is left out: it is missing from the map.'
	);
	await expect(notice).toContainText(/Line \d+ \(NO SHAPE\): /);
	await expect(notice).toContainText(
		'notes.txt: This is not an airspace file the app can read. It reads OpenAir files, which usually end in .txt.'
	);
	await keep('34-airspace-import');
	await notice.getByRole('button', { name: 'OK' }).click();

	// What the file left out stays on its row for as long as the file is there.
	const files = page.getByRole('list', { name: 'Your files' });
	const zones = files.getByRole('listitem').filter({ hasText: 'test-zones' });
	await expect(zones).toContainText(/2 airspaces · brought in \w{3}, \d+ \w{3} \d{4}/);
	await expect(zones).toContainText(
		'1 airspace in this file could not be read. It is missing from the map.'
	);

	// A file dropped on this screen is an airspace file.
	await dropFiles(
		page,
		[{ name: 'more.txt', text: ZONES.replace('CTR TEST', 'CTR MORE').replace('011:44', '011:50') }],
		'Drop airspace files to bring them in'
	);
	await page.getByRole('dialog', { name: 'Airspace' }).getByRole('button', { name: 'OK' }).click();
	await expect(files.getByRole('listitem').filter({ hasText: 'more' })).toBeVisible();
	if (wide) await page.mouse.move(0, 0);
	await keep('35-airspace');

	// A file is removed after asking.
	await files.getByRole('button', { name: 'Remove more' }).click();
	const asking = page.getByRole('alertdialog', { name: 'Remove this file?' });
	await expect(asking).toContainText('Its airspace is no longer drawn.');
	await asking.getByRole('button', { name: 'Remove file' }).click();
	await expect(files.getByRole('listitem').filter({ hasText: 'more' })).toBeHidden();
	await sections(page).getByRole('link', { name: 'Settings', exact: true }).click();
	await expect(page.getByRole('link', { name: /^Airspace/ })).toHaveText(
		/openAIP, \d+ airspaces · 1 file/
	);

	// Switched off, the server's airspace is deleted and no longer asked for;
	// the pilot's own file is still drawn.
	await page.getByRole('link', { name: /^Airspace/ }).click();
	await fromTheServer.uncheck();
	await expect(page.getByText('Switched off: only your own files are used.')).toBeVisible();
	await expect(fromServer).not.toContainText('openAIP');
	await sections(page).getByRole('link', { name: 'Map', exact: true }).click();
	await mapTabDrawn(page, mapDrawn);
	await expect(drawn(page)).toHaveText('Airspace on this part of the map: 2.');
	await expect(page.getByText(/^Airspace: /)).toBeHidden();

	// The choice is remembered, and undone with the same switch.
	await page.reload();
	await mapTabDrawn(page, mapDrawn);
	await expect(page.getByText(/^Airspace: /)).toBeHidden();
	await openAirspace(page);
	await expect(fromTheServer).not.toBeChecked();
	await fromTheServer.check();
	await expect(page.getByText('Fetched the next time a map is open')).toBeVisible();
	await sections(page).getByRole('link', { name: 'Map', exact: true }).click();
	await expect(page.getByText('Airspace: openAIP, not certified')).toBeVisible();
});

test('the map of a flight draws the airspace around it', async ({ page, mapDrawn, keep }) => {
	await page.goto('/settings/airspace/');
	await addFiles(page, [upload('Test Zones.txt', ZONES)]);
	await page.getByRole('dialog', { name: 'Airspace' }).getByRole('button', { name: 'OK' }).click();

	await addSampleFlight(page);

	// The control zone of the file lies over the valley the flight crosses,
	// named with its floor and ceiling; the server's airspace is around it.
	await expect(drawn(page)).toHaveText(
		/^Airspace on this part of the map, as it is today: [1-9]\d*\.$/
	);
	await expect(page.getByText('Airspace: openAIP, not certified')).toBeVisible();
	await mapDrawn();
	await page.waitForTimeout(1000);
	await keep('36-flight-airspace');
});

test('a site without a server draws the pilot’s own files and credits nobody', async ({
	context,
	page,
	mapDrawn,
	keep
}) => {
	// What a web server answers that only hands out the app's files.
	await context.route('**/v1/**', (route) => route.fulfill({ status: 404, body: 'Not found' }));
	await page.goto('/settings/airspace/');
	await expect(
		page.getByText('This site has no Venti server behind it, so there is no airspace to fetch.')
	).toBeVisible();
	await expect(page.getByRole('switch')).toBeHidden();

	await addFiles(page, [upload('Test Zones.txt', ZONES)]);
	await page.getByRole('dialog', { name: 'Airspace' }).getByRole('button', { name: 'OK' }).click();
	await keep('37-airspace-no-server');

	await sections(page).getByRole('link', { name: 'Map', exact: true }).click();
	await mapDrawn();
	await expect(drawn(page)).toHaveText('Airspace on this part of the map: 2.');
	await expect(page.getByText(/^Airspace: /)).toBeHidden();

	// Closer in, airspace to stay out of is named, with its floor and its
	// ceiling under the name.
	await context.grantPermissions(['geolocation']);
	await context.setGeolocation(IN_THE_VALLEY);
	await page.getByRole('button', { name: 'Show my position' }).click();
	await mapDrawn();
	await expect(drawn(page)).toHaveText('Airspace on this part of the map: 1.');
	await page.waitForTimeout(1000);
	await keep('38-map-airspace-names');
});

test('a server that cannot be asked for airspace is named on the airspace screen, and left alone', async ({
	context,
	page,
	mapDrawn
}) => {
	let asked = 0;
	await context.route('**/v1/airspace**', (route) => {
		asked += 1;
		return route.abort();
	});
	await page.goto('/');
	await mapTabDrawn(page, mapDrawn);
	// The map says nothing: it simply has no airspace.
	await expect(page.getByText(/^Airspace: /)).toBeHidden();

	await openAirspace(page);
	await expect(page.getByRole('status')).toHaveText(
		'The server could not be reached for airspace.'
	);

	// Back on the map, the server is not asked again at once.
	await sections(page).getByRole('link', { name: 'Map', exact: true }).click();
	await mapTabDrawn(page, mapDrawn);
	expect(asked).toBe(1);
});
