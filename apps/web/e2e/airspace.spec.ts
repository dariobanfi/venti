/**
 * Airspace on the maps: what the app fetches from the Venti server around
 * where the pilot looks and keeps in the browser, and the OpenAir files the
 * pilot brings in.
 *
 * The server is the development mode's: openAIP's airspace of the northern
 * Alps as recorded, among it the terminal area of Innsbruck, which reaches
 * over the valley of the sample flight.
 */

import type { Locator, Page } from '@playwright/test';
import {
	addSampleFlight,
	dropFiles,
	expect,
	flightDetails,
	IN_THE_VALLEY,
	mapTabDrawn,
	metric,
	panel,
	test,
	openCredits,
	airspaceButton,
	closeLayers,
	layerSwitch,
	openLayers,
	switchAirspaceOn
} from './helpers';

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

/**
 * Two airspaces the sample flight meets: a control zone whose edge its
 * second thermal drifts into, and a restricted area over the whole valley
 * whose floor is a height above the ground, which the app cannot place.
 */
const ENTERED = `AC D
AY CTR
AN CTR TEGERNSEE
AL GND
AH 9000ft AMSL
V X=47:42:00 N 011:44:00 E
DC 1

AC R
AN ED-R VALLEY
AL 1000ft AGL
AH FL 100
DP 47:30:00 N 011:30:00 E
DP 47:30:00 N 012:00:00 E
DP 47:50:00 N 012:00:00 E
DP 47:50:00 N 011:30:00 E
`;

const upload = (name: string, text: string) => ({
	name,
	mimeType: 'application/octet-stream',
	buffer: Buffer.from(text)
});

/** How many airspaces a map says it draws, for those who cannot see it. */
const drawn = (page: Page) => page.getByText(/^Airspace on this part of the map/);

/** Brings what a flight's details say about airspace to the top of what scrolls. */
const showAirspacePart = (details: Locator) =>
	details
		.getByRole('heading', { name: 'Airspace' })
		.evaluate((heading) => heading.scrollIntoView({ block: 'start' }));

const sections = (page: Page) => page.getByRole('navigation', { name: 'Sections' });

/** Opens the screen that lists the pilot's airspace, from wherever the navigation shows. */
async function openAirspace(page: Page): Promise<void> {
	await sections(page).getByRole('link', { name: 'Settings', exact: true }).click();
	await page
		.locator('.settings-page')
		.getByRole('link', { name: /^Airspace/ })
		.click();
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
	await page.goto('/map/');
	await mapTabDrawn(page, mapDrawn);
	await switchAirspaceOn(page);
	await mapDrawn();

	// Fetched around what the map shows, kept, and drawn: no file was needed.
	await expect(drawn(page)).toHaveText(/^Airspace on this part of the map: [1-9]\d*\.$/);
	// openAIP's licence asks for its credit and its notice wherever its
	// airspace is shown. On the map that is a line; the airspace screen has
	// it in full.
	await openCredits(page);
	await expect(page.getByText('Airspace: openAIP, not certified')).toBeVisible();
	await openCredits(page);
	await expect(page.getByText(/^Wind: .*Deutscher Wetterdienst.*GeoSphere Austria/)).toBeVisible();
	await mapDrawn();
	await keep('33-map-airspace');

	// Kept in the browser: opened again without the server's airspace, the
	// map still has it.
	await context.route('**/v1/airspace**', (route) => route.abort());
	await page.reload();
	await mapTabDrawn(page, mapDrawn);
	await expect(drawn(page)).toHaveText(/: [1-9]\d*\.$/);
	await openCredits(page);
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
	await page.goto('/map/');
	await mapTabDrawn(page, mapDrawn);
	await switchAirspaceOn(page);
	await openCredits(page);
	await expect(page.getByText('Airspace: openAIP, not certified')).toBeVisible();
	await sections(page).getByRole('link', { name: 'Settings', exact: true }).click();
	const row = page.locator('.settings-page').getByRole('link', { name: /^Airspace/ });
	await expect(row).toHaveText(/openAIP, \d+ airspaces/);
	await row.click();
	await expect(page).toHaveTitle('Airspace · Venti');

	// What is kept from the server: how much, how old, of which countries,
	// and what its provider asks to be said about it.
	const fromServer = page.getByRole('list', { name: 'From Venti' });
	await expect(fromServer).toContainText('openAIP');
	await expect(fromServer).toContainText(/\d+ airspaces · as of \w{3}, \d+ \w{3} \d{4} · fetched /);
	await expect(fromServer).toContainText(
		'The airspace of Germany, Austria, Switzerland, France, Italy, Slovenia, Croatia, Bosnia & Herzegovina, Hungary and Slovakia. Of other countries none is kept'
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
	const fromTheServer = page.getByRole('switch', { name: 'Airspace from Venti' });
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
	await expect(page.locator('.settings-page').getByRole('link', { name: /^Airspace/ })).toHaveText(
		/openAIP, \d+ airspaces · 1 file/
	);

	// Switched off, the server's airspace is deleted and no longer asked for;
	// the pilot's own file is still drawn.
	await page
		.locator('.settings-page')
		.getByRole('link', { name: /^Airspace/ })
		.click();
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
	await openCredits(page);
	await expect(page.getByText('Airspace: openAIP, not certified')).toBeVisible();
});

test('the map of a flight draws the airspace around it', async ({ page, mapDrawn, keep }) => {
	await page.goto('/settings/airspace/');
	await addFiles(page, [upload('Test Zones.txt', ZONES)]);
	await page.getByRole('dialog', { name: 'Airspace' }).getByRole('button', { name: 'OK' }).click();

	await addSampleFlight(page);
	// The map counts its airspace once it has drawn everything, which a
	// phone's map, drawn in software at twice the density, takes its time
	// over on a busy machine.
	await mapDrawn();

	// The control zone of the file lies over the valley the flight crosses,
	// named with its floor and ceiling; the server's airspace is around it.
	await expect(drawn(page)).toHaveText(
		/^Airspace on this part of the map, as it is today: [1-9]\d*\.$/
	);
	await openCredits(page);
	await expect(page.getByText('Airspace: openAIP, not certified')).toBeVisible();
	await mapDrawn();
	await page.waitForTimeout(1000);
	await keep('36-flight-airspace');
});

// The core checks a flight of its own store only, and the website keeps no
// flights of its own (owner decision 20): this waits for the core to check a
// flight from its file, as it reads everyone's.
test.fixme('a flight’s details list the airspace it entered, and say what that is worth', async ({
	page,
	mapDrawn,
	keep,
	wide
}) => {
	await page.goto('/settings/airspace/');
	await addFiles(page, [upload('Entered.txt', ENTERED)]);
	await page.getByRole('dialog', { name: 'Airspace' }).getByRole('button', { name: 'OK' }).click();
	await addSampleFlight(page);

	// Everything else known about the flight: a sheet on a phone, on screen
	// all the time where there is room.
	const details = flightDetails(page);
	const sheet = page.getByRole('dialog', { name: 'Flight details' });
	if (!wide) await page.getByRole('button', { name: 'Flight details' }).click();
	const entered = details.getByRole('list', { name: 'Airspace' }).getByRole('button');

	// The whole flight lies in the restricted area's outline, but whether
	// above its floor the app cannot tell: "possibly", in the core's words.
	const restricted = entered.filter({ hasText: 'ED-R VALLEY' });
	await expect(restricted).toContainText('Restricted area · 1000 ft AGL to FL 100');
	await expect(restricted).toContainText('12:30 · 786 to 2117 m');
	await expect(restricted).toContainText(
		'Possibly clear: the app does not know the height of the ground there.'
	);
	await expect(restricted).toContainText('29:04 min');

	// A minute at the edge of the control zone, twelve minutes into the
	// flight: when, how long, and between which heights.
	const zone = entered.filter({ hasText: 'CTR TEGERNSEE' });
	await expect(zone).toContainText('Class D control zone · GND to 9000 ft AMSL');
	await expect(zone).toContainText(/12:4\d · [\d,]+ to [\d,]+ m/);
	await expect(zone).toContainText(/[12]:\d\d min/);
	await expect(zone).not.toContainText('Possibly');

	// Under the list: what the check is worth.
	await expect(details).toContainText(
		'Checked against the airspace the app has today, not the airspace of the day of the flight, and without knowing what was active. A help for your own review, nothing more.'
	);
	await showAirspacePart(details);
	await mapDrawn();
	await keep('45-flight-airspace-entered');

	// Choosing a stretch marks the moment the glider entered, on the map
	// and the profile.
	await zone.click();
	if (!wide) await expect(sheet).toBeHidden();
	await expect(metric(panel(page).moment, 'Time')).toHaveText(/^12:4\d$/);
	await mapDrawn();
	await keep('46-flight-airspace-entry');
});

// The core checks a flight of its own store only, and the website keeps no
// flights of its own (owner decision 20): this waits for the core to check a
// flight from its file, as it reads everyone's.
test.fixme('a flight that entered nothing says so, and a flight nothing could be checked against too', async ({
	context,
	page,
	keep,
	wide
}) => {
	// With the server's airspace alone: the terminal area of Innsbruck lies
	// far above the flight.
	await addSampleFlight(page);
	const details = flightDetails(page);
	if (!wide) await page.getByRole('button', { name: 'Flight details' }).click();
	await expect(details).toContainText('The flight entered none of the airspace the app has.');
	await expect(details).toContainText('Checked against the airspace the app has today');
	await showAirspacePart(details);
	await keep('47-flight-airspace-none-entered');

	// Without any airspace nothing was checked, which is not "nothing entered".
	await page.goto('/settings/airspace/');
	await page.getByRole('switch', { name: 'Airspace from Venti' }).uncheck();
	await expect(page.getByText('Switched off: only your own files are used.')).toBeVisible();
	await context.route('**/v1/airspace**', (route) => route.abort());
	await page.goto('/logbook/');
	await page.getByRole('link', { name: /Sat, 4 Jul/ }).click();
	if (!wide) await page.getByRole('button', { name: 'Flight details' }).click();
	await expect(details).toContainText(
		'Not checked: the app has no airspace where this flight was flown.'
	);
	await expect(details).toContainText(
		'Bring in an airspace file under Settings, Airspace, and the flight is checked against what the app then has.'
	);
	await showAirspacePart(details);
	await keep('48-flight-airspace-not-checked');
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
		page.getByText(
			'This copy of the app is served without Venti behind it, so there is no airspace to fetch.'
		)
	).toBeVisible();
	await expect(page.getByRole('switch')).toBeHidden();

	await addFiles(page, [upload('Test Zones.txt', ZONES)]);
	await page.getByRole('dialog', { name: 'Airspace' }).getByRole('button', { name: 'OK' }).click();
	await keep('37-airspace-no-server');

	await sections(page).getByRole('link', { name: 'Map', exact: true }).click();
	await switchAirspaceOn(page);
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

/**
 * Two airspaces over the valley: a control zone from the ground, and an
 * area whose floor, FL 100, certainly lies above 2500 m.
 */
const OVER_THE_VALLEY = `AC D
AY CTR
AN CTR VALLEY
AL GND
AH 9000ft AMSL
V X=47:40:55 N 011:45:54 E
DC 2

AC R
AN ED-R HIGH VALLEY
AL FL 100
AH FL 150
DP 47:35:00 N 011:35:00 E
DP 47:35:00 N 011:55:00 E
DP 47:48:00 N 011:55:00 E
DP 47:48:00 N 011:35:00 E
`;

test('a click on the map says what airspace lies over the place, and the height filter thins every map', async ({
	context,
	page,
	mapDrawn,
	keep,
	wide,
	problems
}) => {
	// The pilot's own file alone, on a site without a server: what the map
	// draws is known to the airspace.
	await context.route('**/v1/**', (route) => route.fulfill({ status: 404, body: 'Not found' }));
	await page.goto('/settings/airspace/');
	await addFiles(page, [upload('Over the valley.txt', OVER_THE_VALLEY)]);
	await page.getByRole('dialog', { name: 'Airspace' }).getByRole('button', { name: 'OK' }).click();
	await sections(page).getByRole('link', { name: 'Map', exact: true }).click();
	await switchAirspaceOn(page);
	await context.grantPermissions(['geolocation']);
	await context.setGeolocation(IN_THE_VALLEY);
	await page.getByRole('button', { name: 'Show my position' }).click();
	await mapDrawn();
	await expect(drawn(page)).toHaveText('Airspace on this part of the map: 2.');

	// A click where nothing else of the map's lies: what lies over the place,
	// from the lowest floor up, in the core's words.
	const canvas = page.locator('canvas.maplibregl-canvas');
	const box = (await canvas.boundingBox())!;
	await canvas.click({ position: { x: box.width / 2 + 40, y: box.height / 3 } });
	const sheet = page.getByRole(wide ? 'complementary' : 'dialog', { name: /° N, .*° E$/ });
	await expect(sheet).toBeVisible();
	const listed = sheet.getByRole('list', { name: 'Airspace over this place' }).getByRole('heading');
	await expect(listed).toHaveText(['CTR VALLEY', 'ED-R HIGH VALLEY']);
	await expect(sheet).toContainText('not certified');
	await expect(sheet).toContainText('not a clearance');
	const heights = sheet.getByRole('radiogroup', { name: 'Airspace on the map' });
	await expect(heights.getByRole('radio', { name: 'All', exact: true })).toBeChecked();
	await keep('118-map-airspace-over');

	// Up to 2500 m: the area above it leaves the map; the zone from the
	// ground stays.
	await heights.locator('label', { hasText: '2500 m' }).click();
	await expect(heights.getByRole('radio', { name: '2500 m' })).toBeChecked();
	await mapDrawn();
	await expect(drawn(page)).toHaveText('Airspace on this part of the map: 1.');
	await keep('119-map-airspace-filtered');

	// Kept, as the aerial view is.
	await page.reload();
	await page.getByRole('button', { name: 'Show my position' }).click();
	await mapDrawn();
	await expect(drawn(page)).toHaveText('Airspace on this part of the map: 1.');
	expect(problems).toEqual([]);
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
	await page.goto('/map/');
	await mapTabDrawn(page, mapDrawn);
	await switchAirspaceOn(page);
	// The map says nothing: it simply has no airspace.
	await expect(page.getByText(/^Airspace: /)).toBeHidden();

	await openAirspace(page);
	await expect(page.getByRole('status')).toHaveText('Venti could not be reached for airspace.');

	// Back on the map, the server is not asked again at once.
	await sections(page).getByRole('link', { name: 'Map', exact: true }).click();
	await mapTabDrawn(page, mapDrawn);
	expect(asked).toBe(1);
});

test('the map tab opens without airspace, and one round button switches it on and off', async ({
	page,
	mapDrawn,
	keep,
	problems
}) => {
	let asked = 0;
	page.on('request', (request) => {
		if (new URL(request.url()).pathname.startsWith('/v1/airspace')) asked += 1;
	});
	await page.goto('/map/');
	await mapTabDrawn(page, mapDrawn);
	await mapDrawn();

	// Off as the map opens: nothing drawn, nothing asked for, nobody credited.
	const button = airspaceButton(page);
	await expect(button).toHaveAttribute('aria-pressed', 'false');
	await expect(page.locator('.controls > button')).toHaveCount(3);
	await expect(drawn(page)).toBeHidden();
	expect(asked).toBe(0);
	await openCredits(page);
	await expect(page.getByText(/^Airspace: /)).toBeHidden();
	await openCredits(page);
	await page.waitForTimeout(800);
	await keep('277-map-airspace-off');

	// One tap: drawn, credited, and the layers' row is on with it.
	await button.click();
	await expect(button).toHaveAttribute('aria-pressed', 'true');
	await mapDrawn();
	await expect(drawn(page)).toHaveText(/^Airspace on this part of the map: [1-9]\d*\.$/);
	await openLayers(page);
	await expect(layerSwitch(page, 'Airspace')).toBeChecked();
	await closeLayers(page);
	await page.waitForTimeout(800);
	await keep('278-map-airspace-on');

	// Too far out to draw any, the map says so in the core's words.
	const zoomIn = page.getByText('Zoom in to see airspace.');
	await expect(zoomIn).toBeHidden();
	const canvas = page.locator('canvas.maplibregl-canvas');
	await canvas.focus();
	for (let step = 0; step < 8 && !(await zoomIn.isVisible()); step += 1) {
		await page.keyboard.press('Minus');
		await mapDrawn();
	}
	await expect(zoomIn).toBeVisible();
	await canvas.blur();

	// Kept by the browser; off again by the layers' row, which the button follows.
	await page.reload();
	await mapTabDrawn(page, mapDrawn);
	await expect(airspaceButton(page)).toHaveAttribute('aria-pressed', 'true');
	await openLayers(page);
	await layerSwitch(page, 'Airspace').uncheck();
	await closeLayers(page);
	await expect(airspaceButton(page)).toHaveAttribute('aria-pressed', 'false');
	await mapDrawn();
	await expect(drawn(page)).toBeHidden();
	expect(problems).toEqual([]);
});
