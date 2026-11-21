/**
 * A pilot adds a flying site to Venti's own layer, corrects it, looks at
 * its history, undoes the correction and removes the site; and is told
 * when a site stands at the place already. The server is the development
 * mode's, which takes sites from pilots and has four made-up ones near
 * Garmisch, where the test map is blank.
 *
 * A site is written from an account, and no two sites of the layer may
 * stand within 20 m of each other. Every test makes an account of its own
 * and a site at a place of its own, so that tests running side by side on
 * one server do not stand in each other's way.
 */

import type { Locator, Page } from '@playwright/test';
import { expect, mapTabDrawn, metric, showMyPosition, test } from './helpers';

/** The development mode's made-up take-off that pilots made. */
const KOGEL = { latitude: 47.3, longitude: 10.9 };

const GIVING =
	'You give this site to everyone under CC0 1.0, the public domain: anyone may use it and change it, on Venti and anywhere else. It is shown with your handle, any pilot can correct it, and it stays on Venti when your account goes.';

const field = (page: Page, name: string) => page.getByRole('textbox', { name });

/** A panel beside the map on a wide screen, a sheet on a narrow one. */
function panel(page: Page, wide: boolean, name: string): Locator {
	return page.getByRole(wide ? 'complementary' : 'dialog', { name });
}

/** Creates an account nobody has used on this server, and leaves the browser signed in to it. */
async function signUp(page: Page): Promise<string> {
	const handle = `pilot${Math.random().toString(36).slice(2, 10)}`;
	await page.goto('/settings/account/');
	await page.getByRole('radio', { name: 'New account' }).check();
	await field(page, 'Handle').fill(handle);
	await field(page, 'Name shown to others').fill('Test Pilot');
	await field(page, 'Password').fill('correct horse battery staple');
	await page.getByRole('button', { name: 'Create account' }).click();
	await expect(metric(page, 'Handle')).toHaveText(`@${handle}`);
	return handle;
}

/** The identifiers of the signs the map draws. */
const signs = (page: Page) =>
	page.evaluate(
		() => (globalThis as unknown as { ventiSites: { signs: string[] } }).ventiSites.signs
	);

/**
 * Opens the form for a new site at the place the pilot stands at: "Add a
 * site" puts a mark over the middle of the map, and "Use this place" takes it.
 */
async function addHere(page: Page, wide: boolean): Promise<Locator> {
	await page.getByRole('button', { name: 'Add a site' }).click();
	const placing = page.getByRole('group', { name: 'Add a site' });
	await expect(placing).toContainText(
		'Move the map until the mark is where pilots stand or touch down.'
	);
	await placing.getByRole('button', { name: 'Use this place' }).click();
	const form = panel(page, wide, 'Add a site');
	await expect(form).toBeVisible();
	return form;
}

test('a pilot adds a take-off, corrects its height, undoes that from its history and removes it', async ({
	context,
	page,
	mapDrawn,
	keep,
	wide,
	problems
}) => {
	// South of the made-up sites, more than a kilometre from each, and a
	// place of this run's own.
	const here = {
		latitude: 47.255 + Math.random() * 0.012,
		longitude: 10.86 + Math.random() * 0.08
	};
	const name = `Testkogel ${Math.random().toString(36).slice(2, 7)}`;
	const handle = await signUp(page);
	await context.grantPermissions(['geolocation']);
	await context.setGeolocation(here);
	await page.goto('/');
	await mapTabDrawn(page, mapDrawn);
	await showMyPosition(page, mapDrawn);

	// The entry on the map: the server takes sites, so the map offers to add one.
	await page.getByRole('button', { name: 'Add a site' }).click();
	const placing = page.getByRole('group', { name: 'Add a site' });
	await expect(placing).toContainText(
		'Move the map until the mark is where pilots stand or touch down.'
	);
	await keep('160-site-add-on-map');
	await placing.getByRole('button', { name: 'Use this place' }).click();
	const form = panel(page, wide, 'Add a site');
	await expect(form).toBeVisible();
	// The fields in the core's order, each under its title.
	await expect(form.locator('.group-title')).toHaveText([
		'Name',
		'Place',
		'Used for',
		'Height',
		'Wind directions',
		'Description',
		'Fee',
		'Rules for guests',
		'Not to be used',
		'Web page',
		'Why'
	]);
	await expect(
		form.getByRole('group', { name: 'Wind directions' }).getByRole('button')
	).toHaveCount(16);
	await keep('161-site-form-empty');
	await page.emulateMedia({ colorScheme: 'dark' });
	await keep('161-site-form-empty-dark');
	await page.emulateMedia({ colorScheme: 'light' });

	// Sent as it is, nothing goes to the server: the core's faults stand at their fields.
	const send = form.getByRole('button', { name: 'Add for everyone' });
	await send.click();
	await expect(form.getByRole('alert').filter({ hasText: 'Give the site a name.' })).toBeVisible();
	await form.getByRole('textbox', { name: 'Name' }).scrollIntoViewIfNeeded();
	await keep('162-site-form-fault');

	await form.getByRole('textbox', { name: 'Name' }).fill(name);
	await expect(form.getByText('Give the site a name.')).toBeHidden();
	await form.getByRole('button', { name: 'Take-off' }).click();
	await form.getByRole('spinbutton', { name: 'Height' }).fill('1500');
	const winds = form.getByRole('group', { name: 'Wind directions' });
	await winds.getByRole('button', { name: 'SW', exact: true }).click();
	await winds.getByRole('button', { name: 'WSW', exact: true }).click();
	// The wind as the core will send it, under the points.
	await expect(form.getByText('SW to WSW', { exact: true })).toBeVisible();
	await form.getByRole('textbox', { name: 'Why' }).fill('Made by a test.');
	await expect(form.getByRole('alert')).toHaveCount(0);

	// What giving a site means stands over the button, with the licence to read.
	await expect(form.getByText(GIVING)).toBeVisible();
	const licence = form.getByRole('link', { name: 'CC0 1.0' });
	await expect(licence).toHaveAttribute('href', /^https:\/\/creativecommons\.org\//);
	await send.scrollIntoViewIfNeeded();
	await keep('163-site-form-giving');
	await send.click();

	// The core's line, and the new site's page.
	const site = panel(page, wide, name);
	await expect(site).toBeVisible();
	await expect(site.getByRole('status')).toHaveText(
		'Added. The site is on the map for everyone now.'
	);
	await expect(metric(site, 'What it is')).toHaveText('Take-off');
	await expect(metric(site, 'Height')).toHaveText(/^1500\sm$/);
	await expect(metric(site, 'Wind it works in')).toContainText('SW to WSW');
	const by = site.getByRole('region', { name: 'Added by pilots' });
	await expect(by).toContainText(`Added by Test Pilot (@${handle})`);
	await expect(by).toContainText(
		'Added by pilots on Venti and not verified. A site on the map is not permission to fly there.'
	);
	// It stands on the map, among the signs.
	const before = await signs(page);
	expect(before.filter((id) => id.startsWith('venti:')).length).toBeGreaterThan(0);
	await keep('164-site-added');

	// Corrected: the form on the site as it stands.
	await by.getByRole('button', { name: 'Correct this site' }).click();
	const correcting = panel(page, wide, 'Correct this site');
	await expect(correcting.getByRole('textbox', { name: 'Name' })).toHaveValue(name);
	await correcting.getByRole('spinbutton', { name: 'Height' }).fill('1520');
	await correcting.getByRole('textbox', { name: 'Why' }).fill('Measured again.');
	await correcting.getByRole('button', { name: 'Save for everyone' }).click();
	await expect(site.getByRole('status')).toHaveText(
		'Saved for everyone, as revision 2 of this site.'
	);
	await expect(metric(site, 'Height')).toHaveText(/^1520\sm$/);
	await expect(by).toContainText(`Last changed by Test Pilot (@${handle})`);

	// Its history: two revisions, the newest first, with the change in words.
	await by.getByRole('button', { name: 'History' }).click();
	const history = page.getByRole('dialog', { name: 'History' });
	const revisions = history.getByRole('list', { name: 'Revisions, the newest first' });
	await expect(revisions.getByRole('listitem')).toHaveCount(2);
	await expect(revisions.getByRole('listitem').first()).toContainText(
		`Changed by Test Pilot (@${handle})`
	);
	await expect(revisions.getByRole('listitem').first()).toContainText('“Measured again.”');
	await expect(revisions.getByRole('listitem').first()).toContainText('Height: 1500 m → 1520 m');
	await expect(revisions.getByRole('listitem').last()).toContainText(
		`Added by Test Pilot (@${handle})`
	);
	await keep('165-site-history');

	// Undone: the revision before is put back, as a third one.
	await history.getByRole('button', { name: 'Undo the last change' }).click();
	const why = page.getByRole('alertdialog', { name: 'Undo the last change' });
	await why.getByRole('textbox', { name: 'Why' }).fill('The first height was right.');
	await why.getByRole('button', { name: 'Undo the last change' }).click();
	await expect(revisions.getByRole('listitem')).toHaveCount(3);
	await expect(revisions.getByRole('listitem').first()).toContainText('Put back to revision 1');
	await expect(revisions.getByRole('listitem').first()).toContainText('Height: 1520 m → 1500 m');
	await history.getByRole('button', { name: 'Done' }).click();
	await expect(metric(site, 'Height')).toHaveText(/^1500\sm$/);

	// Removed, with a line about it: off the map, and its history stays in reach.
	await by.getByRole('button', { name: 'Remove this site' }).click();
	const removing = page.getByRole('alertdialog', { name: 'Remove this site' });
	await removing.getByRole('textbox', { name: 'Why' }).fill('It was a test.');
	await removing.getByRole('button', { name: 'Remove', exact: true }).click();
	await expect(history.getByRole('status').first()).toHaveText(
		'Removed from the map. Its history stays, and any pilot can bring it back.'
	);
	await expect(revisions.getByRole('listitem')).toHaveCount(4);
	await expect(revisions.getByRole('listitem').first()).toContainText('“It was a test.”');
	// Any revision that says how the site stood can bring it back.
	await expect(revisions.getByRole('button', { name: 'Put back to this' }).first()).toBeVisible();
	await expect(site).toBeHidden();
	await keep('166-site-removed-history');
	expect((await signs(page)).length).toBe(before.length - 1);

	expect(problems).toEqual([]);
});

test('a site placed on top of another is refused, with the offer to open that one', async ({
	context,
	page,
	mapDrawn,
	keep,
	wide,
	problems
}) => {
	await signUp(page);
	await context.grantPermissions(['geolocation']);
	await context.setGeolocation(KOGEL);
	await page.goto('/');
	await mapTabDrawn(page, mapDrawn);
	await showMyPosition(page, mapDrawn);

	const form = await addHere(page, wide);
	await form.getByRole('textbox', { name: 'Name' }).fill('Musterkogel again');
	await form.getByRole('button', { name: 'Take-off' }).click();
	await form.getByRole('button', { name: 'Add for everyone' }).click();

	// The core's refusal names the site that stands there, and offers it.
	const refusal = form.getByRole('alert');
	await expect(refusal).toContainText(
		'A site that pilots added stands at this place already: Musterkogel West. If it is the same place, correct that one.'
	);
	const open = refusal.getByRole('button', { name: 'Open Musterkogel West' });
	await open.scrollIntoViewIfNeeded();
	await keep('167-site-too-close');
	await open.click();

	const site = panel(page, wide, 'Musterkogel West');
	await expect(site).toBeVisible();
	await expect(metric(site, 'Height')).toHaveText(/^1862\sm$/);
	const by = site.getByRole('region', { name: 'Added by pilots' });
	await expect(by).toContainText('Last changed by');
	await expect(by.getByRole('button', { name: 'Correct this site' })).toBeVisible();
	await expect(by.getByRole('button', { name: 'Report this site' })).toBeVisible();

	expect(problems).toEqual([]);
});

test('a pilot who is signed out is told why adding a site needs an account', async ({
	page,
	mapDrawn,
	problems
}) => {
	await page.goto('/');
	await mapTabDrawn(page, mapDrawn);

	await page.getByRole('button', { name: 'Add a site' }).click();

	const told = page.getByRole('dialog', { name: 'Add a site' });
	await expect(told).toContainText('A site is added and corrected from your account');
	await expect(told.getByRole('link', { name: 'Account' })).toHaveAttribute(
		'href',
		/\/settings\/account\/?$/
	);
	expect(problems).toEqual([]);
});
