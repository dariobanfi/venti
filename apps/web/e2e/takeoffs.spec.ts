/**
 * What public flights say about where pilots take off, on the development
 * mode's server: its made-up public flights that began at the partner's
 * take-off Musterkogel West, and a place south of it where three pilots
 * took off and no site stands (47.2612 N, 10.9312 E).
 *
 * A site's page has a section of its own, "Public flights from here", with
 * the core's count line and the twelve months; the map draws the places
 * without a site as rings while their switch in the sites' filter is on;
 * a tap on one opens its sheet, which offers a pilot who took off there to
 * add it. That pilot is an account the test makes, with the sample flight
 * moved to start at the place. Every word read is the core's.
 */

import type { Page } from '@playwright/test';
import {
	expect,
	mapTabDrawn,
	metric,
	sampleFlightOn,
	showMyPosition,
	test,
	dropFiles,
	mapTool,
	openSitesFilter
} from './helpers';

/** The made-up place where pilots take off and no site stands. */
const CANDIDATE = { latitude: 47.2612, longitude: 10.9312 };
/** How the map names its ring: its rounded place. */
const CANDIDATE_KEY = `${CANDIDATE.latitude},${CANDIDATE.longitude}`;
/**
 * Where the pilot stands: half a kilometre north of the place, so that the
 * mark of the position does not cover its ring.
 */
const STANDING = { latitude: CANDIDATE.latitude + 0.005, longitude: CANDIDATE.longitude };
/**
 * The sample flight moved so that its track begins at a place: every fix
 * by the same step of latitude and longitude, on a day of its own.
 */
function flightFrom(place: { latitude: number; longitude: number }): string {
	const igc = sampleFlightOn(7);
	const degrees = (whole: string, thousandths: string) =>
		Number(whole.slice(0, -2)) + Number(`${whole.slice(-2)}.${thousandths}`) / 60;
	const written = (value: number, width: number) => {
		const whole = Math.floor(value);
		const minutes = Math.round((value - whole) * 60_000);
		return `${String(whole).padStart(width, '0')}${String(minutes).padStart(5, '0')}`;
	};
	const fix = /^B(\d{6})(\d{4})(\d{3})N(\d{5})(\d{3})E(.*)$/;
	const lines = igc.split(/\r?\n/);
	const first = lines.find((line) => fix.test(line))!.match(fix)!;
	const dLat = place.latitude - degrees(first[2], first[3]);
	const dLon = place.longitude - degrees(first[4], first[5]);
	return lines
		.map((line) => {
			const parts = line.match(fix);
			if (!parts) return line;
			const lat = degrees(parts[2], parts[3]) + dLat;
			const lon = degrees(parts[4], parts[5]) + dLon;
			return `B${parts[1]}${written(lat, 2)}N${written(lon, 3)}E${parts[6]}`;
		})
		.join('\r\n');
}

/** Creates an account nobody has used on this server, and leaves the browser signed in to it. */
async function signUp(page: Page): Promise<void> {
	const handle = `pilot${Math.random().toString(36).slice(2, 10)}`;
	await page.goto('/settings/account/');
	await page.getByRole('radio', { name: 'New account' }).check();
	await page.getByRole('textbox', { name: 'Handle' }).fill(handle);
	await page.getByRole('textbox', { name: 'Name shown to others' }).fill('Test Pilot');
	await page.getByRole('textbox', { name: 'Password' }).fill('correct horse battery staple');
	await page.getByRole('button', { name: 'Create account' }).click();
	await expect(metric(page, 'Handle')).toHaveText(`@${handle}`);
}

// The core finds the pilot's own take-offs in its own store only, and the
// website keeps no flights of its own (owner decision 20): the offer waits for
// the core to be handed the pilot's flights from the server.
test.fixme('a take-off’s page says what public flights began there, and the map offers a place without a site to a pilot who took off there', async ({
	page,
	mapDrawn,
	keep,
	wide,
	problems
}) => {
	await signUp(page);
	// One of the pilot's own flights began at the place.
	await page.goto('/logbook/');
	await dropFiles(page, [{ name: 'from-the-place.igc', text: flightFrom(CANDIDATE) }]);
	await expect(page.getByRole('heading', { name: '7 Jul 2026' })).toBeVisible();

	await page.context().grantPermissions(['geolocation']);
	await page.context().setGeolocation(STANDING);
	await page.goto('/map/');
	await mapTabDrawn(page, mapDrawn);
	await showMyPosition(page, mapDrawn);

	// The take-off's page: the section, its count line and the twelve months.
	await mapTool(page, 'Flying sites');
	await page
		.getByRole('dialog', { name: 'Flying sites' })
		.getByRole('button', { name: 'Musterkogel West' })
		.click();
	const site = page.getByRole(wide ? 'complementary' : 'dialog', { name: 'Musterkogel West' });
	const flights = site.getByRole('region', { name: 'Public flights from here' });
	await expect(flights).toContainText(/\d+ public flights? (by \d+ pilots? )?began here/);
	await expect(flights).toContainText('Public flights that took off within 500 m of this site.');
	const months = flights.getByRole('group', { name: 'When it is flown' });
	await expect(months.getByRole('listitem')).toHaveCount(12);
	await expect(months).toContainText(/January: \d+ flights?/);
	await expect(flights.getByRole('list', { name: 'The best flights from here' })).toBeVisible();
	await flights.scrollIntoViewIfNeeded();
	await keep('210-site-public-flights');
	await page.emulateMedia({ colorScheme: 'dark' });
	await keep('210-site-public-flights-dark');
	await page.emulateMedia({ colorScheme: 'light' });
	await site.getByRole('button', { name: wide ? 'Close' : 'Done' }).click();
	await expect(site).toBeHidden();

	// The layer, off until it is switched on in the sites' filter.
	const filter = await openSitesFilter(page);
	const layer = filter.getByRole('switch', { name: 'Take-offs without a site' });
	await expect(layer).not.toBeChecked();
	await expect(filter).toContainText(
		'Not checked by anyone: a place on the map is not permission to fly there.'
	);
	await layer.check();
	await filter.getByRole('button', { name: 'Done' }).click();
	await expect
		.poll(() =>
			page.evaluate(() => (globalThis as { ventiCandidates?: string[] }).ventiCandidates ?? [])
		)
		.toContain(CANDIDATE_KEY);
	await mapDrawn();
	await keep('211-take-offs-without-a-site');

	// A tap on the ring, where the map says it stands. How far below the
	// pilot's position that is hangs on the zoom, and "Show my position"
	// keeps a zoom that is closer than its own: with a flight in the
	// logbook a wide map opens closer than a phone's.
	const map = (await page.locator('canvas.maplibregl-canvas').boundingBox())!;
	const ring = await page.evaluate(
		(key) =>
			(
				globalThis as unknown as {
					ventiCandidateOnScreen: (id: string) => { x: number; y: number } | undefined;
				}
			).ventiCandidateOnScreen(key),
		CANDIDATE_KEY
	);
	expect(ring).toBeDefined();
	await page.mouse.click(map.x + ring!.x + 4, map.y + ring!.y - 4);
	// Titled by its height: the core's line begins with the core's title.
	const sheet = page.getByRole(wide ? 'complementary' : 'dialog', { name: /^\d+ m$/ });
	await expect(sheet).toContainText(/Pilots take off here: \d+ pilots/);
	await expect(sheet).toContainText('No site names this place.');
	await expect(sheet).toContainText(/You took off here on 7 Jul 2026\./);
	await expect(sheet.getByRole('button', { name: 'Not this one' })).toBeVisible();
	await keep('212-take-off-without-a-site');

	// "Add this place" opens the form of a new site on the core's draft.
	await sheet.getByRole('button', { name: 'Add this place' }).click();
	await expect(
		page.getByRole(wide ? 'complementary' : 'dialog', { name: 'A place pilots take off from' })
	).toBeVisible();

	expect(problems).toEqual([]);
});
