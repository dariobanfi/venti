/**
 * Venti's two tiers of flying sites, on the development mode's server,
 * which takes sites from pilots and has four made-up ones near Garmisch,
 * where the test map is blank: a partner's take-off with two confirmations
 * (Musterkogel West), a crowdsourced take-off with one (Beispieljoch), a
 * partner's landing, and a crowdsourced site that was removed.
 *
 * An account a test makes is an ordinary pilot: it adds "a place pilots
 * take off from" to the crowdsourced tier, which no map shows until its
 * pilot switches "Places from pilots" on, corrects places of that tier,
 * and says "I took off here and it was good" of a site of either tier. The
 * development mode's demo account is its one partner of Venti: it reads
 * "Add a site", adds to the partners' tier, which every map shows, and
 * corrects sites of both. What is offered to whom is the core's answer
 * (`Core.siteOffers`, `Core.siteAdding`); the tests read its words.
 *
 * No two sites of a tier may stand within 20 m of each other, and the same
 * test runs at several widths on one server. A test that adds a site
 * stands at a place of its own, drawn by chance south of the made-up ones,
 * and removes the site again.
 */

import type { Locator, Page } from '@playwright/test';
import { expect, mapTabDrawn, metric, showMyPosition, test } from './helpers';

/** The development mode's made-up take-off of a partner, with two confirmations. */
const KOGEL = { latitude: 47.3, longitude: 10.9 };
/** Its made-up crowdsourced take-off, with one confirmation. */
const JOCH = { latitude: 47.29, longitude: 10.91 };
const JOCH_ID = 'venti-pilots:0000000000000000000000s002';

/** The development mode's one partner of Venti for flying sites. */
const DEMO = { handle: 'demo', password: 'demo-pilot-password' };

/** The development server's clock stands still here; a day confirmed is not after its today. */
const SERVER_NOW = new Date('2026-11-18T19:05:00Z');

/** A part of the core's sentence about giving a place to the crowdsourced tier. */
const GIVING_AS_A_PILOT =
	'It is shown with your handle to pilots who switch on the places from pilots, any pilot can correct it';
/** The entry of someone who has to sign in first, the core's sentence. */
const SIGN_IN_TO_ADD = 'Sign in to add a place pilots take off from.';
const FROM_PILOTS_NOTE =
	'Added by pilots on Venti: not official and not checked by anyone. A site on the map is not permission to fly there.';
const PILOTS_LAYER_ABOUT =
	'Places pilots say they take off from. Not official and not checked by anyone: a place on the map is not permission to fly there.';
const PLACE_WORDS = 'Move the map until the mark is where pilots stand or touch down.';

/** The entry and the form's title for each tier, in the core's words. */
const AS_A_PILOT = {
	entry: 'Add a place pilots take off from',
	title: 'A place pilots take off from',
	send: 'Add as a place from pilots'
};
const AS_A_PARTNER = { entry: 'Add a site', title: 'Add a site', send: 'Add for everyone' };

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

/** Signs in as the development mode's partner. */
async function signInAsPartner(page: Page): Promise<void> {
	await page.goto('/settings/account/');
	await field(page, 'Handle').fill(DEMO.handle);
	await field(page, 'Password').fill(DEMO.password);
	await page.getByRole('button', { name: 'Sign in' }).click();
	await expect(metric(page, 'Handle')).toHaveText(`@${DEMO.handle}`);
}

/** Opens the map tab where the pilot stands. */
async function mapAt(
	page: Page,
	mapDrawn: () => Promise<void>,
	place: { latitude: number; longitude: number }
): Promise<void> {
	await page.context().grantPermissions(['geolocation']);
	await page.context().setGeolocation(place);
	await page.goto('/');
	await mapTabDrawn(page, mapDrawn);
	await showMyPosition(page, mapDrawn);
}

/** The identifiers of the signs the map draws. */
const signs = (page: Page) =>
	page.evaluate(
		() => (globalThis as unknown as { ventiSites: { signs: string[] } }).ventiSites.signs
	);

/** A place of this run's own: south of the made-up sites, more than a kilometre from each. */
const somewhere = () => ({
	latitude: 47.255 + Math.random() * 0.012,
	longitude: 10.86 + Math.random() * 0.08
});

/**
 * Opens the form for a new site at the place the pilot stands at: the
 * tier's entry puts a mark over the middle of the map, and "Use this
 * place" takes it.
 */
async function addHere(page: Page, wide: boolean, as = AS_A_PILOT): Promise<Locator> {
	await page.getByRole('button', { name: as.entry }).click();
	const placing = page.getByRole('group', { name: as.entry });
	await expect(placing).toContainText(PLACE_WORDS);
	await placing.getByRole('button', { name: 'Use this place' }).click();
	const form = panel(page, wide, as.title);
	await expect(form).toBeVisible();
	return form;
}

/** The sites' filter: a pane beside the map on a wide screen, a sheet on a narrow one. */
async function openFilter(page: Page, wide: boolean): Promise<Locator> {
	await page.getByRole('button', { name: 'Filter sites', exact: true }).click();
	const filter = page.getByRole(wide ? 'complementary' : 'dialog', { name: 'Filter sites' });
	await expect(filter).toBeVisible();
	return filter;
}

async function closeFilter(page: Page, wide: boolean, filter: Locator): Promise<void> {
	if (wide) await page.getByRole('button', { name: 'Filter sites', exact: true }).click();
	else await filter.getByRole('button', { name: 'Done' }).click();
	await expect(filter).toBeHidden();
}

/** Opens a site's page from the list of the sites on the map. */
async function openFromList(page: Page, wide: boolean, name: string): Promise<Locator> {
	await page.getByRole('button', { name: 'Flying sites' }).click();
	const list = page.getByRole('dialog', { name: 'Flying sites' });
	await list.getByRole('button', { name }).click();
	const site = panel(page, wide, name);
	await expect(site).toBeVisible();
	return site;
}

test('a pilot adds a place from pilots, which the map shows once that layer is switched on, corrects it and removes it', async ({
	page,
	mapDrawn,
	keep,
	wide,
	problems
}) => {
	const name = `Testkogel ${Math.random().toString(36).slice(2, 7)}`;
	const handle = await signUp(page);
	await mapAt(page, mapDrawn, somewhere());

	// The entry on the map is the crowdsourced tier's: a pilot never reads "Add a site".
	await expect(page.getByRole('button', { name: AS_A_PARTNER.entry, exact: true })).toHaveCount(0);
	await page.getByRole('button', { name: AS_A_PILOT.entry }).click();
	const placing = page.getByRole('group', { name: AS_A_PILOT.entry });
	await expect(placing).toContainText(PLACE_WORDS);
	await keep('160-site-add-on-map');
	await placing.getByRole('button', { name: 'Use this place' }).click();
	const form = panel(page, wide, AS_A_PILOT.title);
	await expect(form).toBeVisible();
	// The tier's notice stands at the head of the form.
	await expect(form.getByText(FROM_PILOTS_NOTE)).toBeVisible();
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
	const send = form.getByRole('button', { name: AS_A_PILOT.send });
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

	// What giving a place to this tier means stands over the button, with the licence to read.
	await expect(form.getByText(GIVING_AS_A_PILOT)).toBeVisible();
	const licence = form.getByRole('link', { name: 'CC0 1.0' });
	await expect(licence).toHaveAttribute('href', /^https:\/\/creativecommons\.org\//);
	await send.scrollIntoViewIfNeeded();
	await keep('163-site-form-giving');
	await send.click();

	// The core's line says where the place is seen, and the new place's page is open.
	const site = panel(page, wide, name);
	await expect(site).toBeVisible();
	await expect(site.getByRole('status').first()).toHaveText(
		'Added. Pilots who switch on the places from pilots see it on the map now.'
	);
	await expect(metric(site, 'What it is')).toHaveText('Take-off');
	await expect(metric(site, 'Height')).toHaveText(/^1500\sm$/);
	await expect(metric(site, 'Wind it works in')).toContainText('SW to WSW');
	// It says its tier by its name, and the tier's notice.
	await expect(site.getByText('From pilots', { exact: true })).toBeVisible();
	await expect(site.getByText(FROM_PILOTS_NOTE)).toBeVisible();
	const by = site.getByRole('region', { name: 'From pilots' });
	await expect(by).toContainText(`Added by Test Pilot (@${handle})`);
	await keep('164-site-added');

	// The layer was not switched on for the pilot: the map shows no place from pilots.
	const fromPilots = async () => (await signs(page)).filter((id) => id.startsWith('venti-pilots:'));
	expect(await fromPilots()).toEqual([]);
	await site.getByRole('button', { name: wide ? 'Close' : 'Done' }).click();
	const filter = await openFilter(page, wide);
	const layer = filter.getByRole('switch', { name: 'Places from pilots' });
	await expect(layer).not.toBeChecked();
	await layer.check();
	await expect.poll(async () => (await fromPilots()).length).toBeGreaterThan(0);
	await closeFilter(page, wide, filter);

	// Now it is on the map and in the list, marked, and opens from there.
	await page.getByRole('button', { name: 'Flying sites' }).click();
	const list = page.getByRole('dialog', { name: 'Flying sites' });
	const row = list.getByRole('listitem').filter({ hasText: name });
	await expect(row).toContainText('From pilots');
	await row.getByRole('button').click();
	await expect(site).toBeVisible();

	// Corrected: the form on the place as it stands, with the tier's words.
	await by.getByRole('button', { name: 'Correct this site' }).click();
	const correcting = panel(page, wide, 'Correct this site');
	await expect(correcting.getByRole('textbox', { name: 'Name' })).toHaveValue(name);
	await expect(correcting.getByText(GIVING_AS_A_PILOT)).toBeVisible();
	await correcting.getByRole('spinbutton', { name: 'Height' }).fill('1520');
	await correcting.getByRole('textbox', { name: 'Why' }).fill('Measured again.');
	await correcting.getByRole('button', { name: 'Save for everyone' }).click();
	await expect(site.getByRole('status').first()).toHaveText(
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
	// The first revision can be put back, in the core's words; the one that stands cannot.
	await expect(revisions.getByRole('button', { name: 'Put back to this revision' })).toHaveCount(1);
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
	const before = await signs(page);
	await by.getByRole('button', { name: 'Remove this site' }).click();
	const removing = page.getByRole('alertdialog', { name: 'Remove this site from the map?' });
	await expect(removing).toContainText(
		'It goes from the map for everyone. Its history stays, and any pilot who may correct the site can bring it back.'
	);
	await removing.getByRole('textbox', { name: 'Why' }).fill('It was a test.');
	await removing.getByRole('button', { name: 'Remove this site' }).click();
	await expect(history.getByRole('status').first()).toHaveText(
		'Removed from the map. Its history stays, and any pilot who may correct the site can bring it back.'
	);
	await expect(revisions.getByRole('listitem')).toHaveCount(4);
	await expect(revisions.getByRole('listitem').first()).toContainText('“It was a test.”');
	// The core's offer for a removed site: to bring it back as it stood before.
	await expect(history.getByRole('button', { name: 'Bring this site back' })).toBeVisible();
	// The revision that removed it is not one to put back; the others bring it back as it stood.
	await expect(
		revisions.getByRole('button', { name: 'Bring the site back as it stood here' })
	).toHaveCount(3);
	await expect(site).toBeHidden();
	await keep('166-site-removed-history');
	expect((await signs(page)).length).toBe(before.length - 1);

	// Brought back, after the core's question: on the map again, as a new revision.
	await history.getByRole('button', { name: 'Bring this site back' }).click();
	const bringing = page.getByRole('alertdialog', { name: 'Bring this site back?' });
	await expect(bringing).toContainText(
		'It is on the map again as it stood before it was removed, as a new revision.'
	);
	await bringing.getByRole('button', { name: 'Bring this site back' }).click();
	await expect(revisions.getByRole('listitem')).toHaveCount(5);
	await expect(history.getByRole('button', { name: 'Bring this site back' })).toHaveCount(0);
	await expect.poll(async () => (await signs(page)).length).toBe(before.length);
	await history.getByRole('button', { name: 'Done' }).click();

	// And removed for good, so that the next run finds the place free.
	const again = await openFromList(page, wide, name);
	await again
		.getByRole('region', { name: 'From pilots' })
		.getByRole('button', { name: 'Remove this site' })
		.click();
	await removing.getByRole('button', { name: 'Remove this site' }).click();
	await expect(history.getByRole('status').first()).toHaveText(/^Removed from the map\./);

	expect(problems).toEqual([]);
});

test('a place put on top of another is refused, with the offer to open that one', async ({
	page,
	mapDrawn,
	keep,
	wide,
	problems
}) => {
	await signUp(page);
	await mapAt(page, mapDrawn, JOCH);

	const form = await addHere(page, wide);
	await form.getByRole('textbox', { name: 'Name' }).fill('Beispieljoch again');
	await form.getByRole('button', { name: 'Take-off' }).click();
	await form.getByRole('button', { name: AS_A_PILOT.send }).click();

	// The core's refusal names the site that stands there, and offers it.
	const refusal = form.getByRole('alert');
	await expect(refusal).toContainText(/^A place from pilots stands here already: Beispieljoch\./);
	const open = refusal.getByRole('button', { name: 'Open Beispieljoch' });
	await open.scrollIntoViewIfNeeded();
	await keep('167-site-too-close');
	await open.click();

	// Its page, though the layer is off: a place from pilots, which a pilot may correct.
	const site = panel(page, wide, 'Beispieljoch');
	await expect(site).toBeVisible();
	await expect(metric(site, 'Height')).toHaveText(/^2104\sm$/);
	const by = site.getByRole('region', { name: 'From pilots' });
	await expect(by).toContainText('Last changed by');
	await expect(by.getByRole('button', { name: 'Correct this site' })).toBeVisible();
	await expect(by.getByRole('button', { name: 'Report this site' })).toBeVisible();
	expect((await signs(page)).filter((id) => id.startsWith('venti-pilots:'))).toEqual([]);

	expect(problems).toEqual([]);
});

test('places from pilots are on the map and in the list only once their switch is on, and marked', async ({
	page,
	mapDrawn,
	keep,
	wide,
	problems
}) => {
	await mapAt(page, mapDrawn, JOCH);

	// Off until the pilot switches it on: neither the map nor the list has the place.
	expect(await signs(page)).not.toContain(JOCH_ID);
	await page.getByRole('button', { name: 'Flying sites' }).click();
	const list = page.getByRole('dialog', { name: 'Flying sites' });
	await expect(list.getByRole('listitem').filter({ hasText: 'Musterkogel West' })).toBeVisible();
	await expect(list.getByRole('listitem').filter({ hasText: 'Beispieljoch' })).toHaveCount(0);
	await list.getByRole('button', { name: 'Done' }).click();

	// The switch stands at the foot of the filter, with the core's sentence.
	const opens = page.getByRole('button', { name: 'Filter sites', exact: true });
	await expect(opens).toHaveAttribute('aria-pressed', 'false');
	const filter = await openFilter(page, wide);
	const layer = filter.getByRole('switch', { name: 'Places from pilots' });
	await expect(layer).not.toBeChecked();
	await expect(filter.getByText(PILOTS_LAYER_ABOUT)).toBeVisible();
	await layer.scrollIntoViewIfNeeded();
	await keep('167b-sites-pilots-switch-off');

	// Switched on: the place is on the map, and the filter's line says how many are from pilots.
	await layer.check();
	await expect.poll(() => signs(page)).toContain(JOCH_ID);
	await expect(filter.getByRole('status')).toContainText(
		/place from pilots: not official and not checked by anyone\.|places from pilots: not official and not checked by anyone\./
	);
	// It counts as a filter that is on.
	await expect(opens).toHaveAttribute('aria-pressed', 'true');
	await layer.scrollIntoViewIfNeeded();
	await keep('167c-sites-pilots-switch-on');
	await closeFilter(page, wide, filter);

	// In the list it says its tier under its name; a partner's site says nothing there.
	await page.getByRole('button', { name: 'Flying sites' }).click();
	await expect(list.getByRole('listitem').filter({ hasText: 'Beispieljoch' })).toContainText(
		'From pilots'
	);
	await expect(
		list.getByRole('listitem').filter({ hasText: 'Musterkogel West' })
	).not.toContainText('From');
	await keep('167d-sites-list-with-pilots');
	await list.getByRole('listitem').filter({ hasText: 'Beispieljoch' }).getByRole('button').click();

	// Its page: the label by the name, the notice above what is said about it,
	// and for a pilot who is signed out the core's line about what signing in is for.
	const site = panel(page, wide, 'Beispieljoch');
	await expect(site.getByText('From pilots', { exact: true })).toBeVisible();
	await expect(site.getByText(FROM_PILOTS_NOTE)).toBeVisible();
	// How many say they took off here, as the core's line. Other runs of these
	// tests confirm the same place on this server, so the number is not read.
	await expect(
		site.getByRole('button', { name: /pilots? took off here, last on \d+ November 2026/ })
	).toBeVisible();
	const by = site.getByRole('region', { name: 'From pilots' });
	await expect(by.getByRole('button', { name: 'History' })).toBeVisible();
	await expect(by.getByRole('button', { name: 'Correct this site' })).toHaveCount(0);
	await expect(site.getByRole('button', { name: 'I took off here and it was good' })).toHaveCount(
		0
	);
	await expect(by).toContainText(
		'Sign in to correct this site, to say that you took off here and to report it.'
	);
	// Signed out, the entry is the core's sentence about signing in, and leads to the account.
	await expect(page.getByRole('button', { name: AS_A_PILOT.entry })).toHaveCount(0);
	await expect(page.getByRole('button', { name: AS_A_PARTNER.entry, exact: true })).toHaveCount(0);
	await expect(page.getByRole('link', { name: SIGN_IN_TO_ADD })).toHaveAttribute(
		'href',
		/\/settings\/account\/?$/
	);

	// The switch is kept as the pilot left it.
	await page.reload();
	await mapTabDrawn(page, mapDrawn);
	await showMyPosition(page, mapDrawn);
	await expect.poll(() => signs(page)).toContain(JOCH_ID);

	expect(problems).toEqual([]);
});

test('a pilot says “I took off here and it was good” of a place from pilots, sees it listed and takes it back', async ({
	page,
	mapDrawn,
	keep,
	wide,
	problems
}) => {
	// The development server's clock stands still: the pilot's yesterday is its 17 November.
	await page.clock.setFixedTime(SERVER_NOW);
	const handle = await signUp(page);
	await mapAt(page, mapDrawn, JOCH);
	const filter = await openFilter(page, wide);
	await filter.getByRole('switch', { name: 'Places from pilots' }).check();
	await expect.poll(() => signs(page)).toContain(JOCH_ID);
	await closeFilter(page, wide, filter);

	const site = await openFromList(page, wide, 'Beispieljoch');
	const confirmed = site.getByRole('region', { name: 'Pilots who took off here' });
	await expect(confirmed).toContainText(/pilots? took off here, last on/);
	await keep('167e-site-from-pilots');
	await page.emulateMedia({ colorScheme: 'dark' });
	await keep('167e-site-from-pilots-dark');
	await page.emulateMedia({ colorScheme: 'light' });

	// The small form: the days as a menu with today chosen, the line, and no flights
	// where the logbook has none that began here.
	await confirmed.getByRole('button', { name: 'I took off here and it was good' }).click();
	const form = page.getByRole('dialog', { name: 'I took off here and it was good' });
	const day = form.getByRole('combobox', { name: 'On which day' });
	await expect(day.locator('option:checked')).toHaveText('Today');
	await expect(form.getByRole('combobox', { name: 'With one of your flights' })).toHaveCount(0);
	await day.selectOption({ label: 'Yesterday' });
	const line = `South-west, easy launch ${Math.random().toString(36).slice(2, 7)}`;
	await form.getByRole('textbox', { name: 'A line for other pilots' }).fill(line);
	await keep('167f-site-confirm-sheet');
	await form.getByRole('button', { name: 'Confirm' }).click();
	await expect(form).toBeHidden();

	// The core's sentence in the button's place, with changing the day and taking it back.
	await expect(site.getByRole('status').first()).toHaveText(
		'You confirmed this site for Tue 17 Nov.'
	);
	await expect(confirmed).toContainText('You confirmed this site for Tue 17 Nov 2026.');
	await expect(confirmed).toContainText(/pilots took off here, last on 17 November 2026/);
	await expect(
		confirmed.getByRole('button', { name: 'I took off here and it was good' })
	).toHaveCount(0);
	await expect(confirmed.getByRole('button', { name: 'Change the day' })).toBeVisible();
	const back = confirmed.getByRole('button', { name: 'Take my confirmation back' });
	await expect(back).toBeVisible();
	await confirmed.scrollIntoViewIfNeeded();
	await keep('167g-site-confirmed');

	// The list of who took off here, with this pilot's line.
	await confirmed.getByRole('button', { name: /took off here, last on/ }).click();
	const who = page.getByRole('dialog', { name: 'Pilots who took off here' });
	await expect(who.getByRole('listitem').filter({ hasText: `(@${handle})` })).toHaveText(
		`Test Pilot (@${handle}) on Tue 17 Nov 2026: “${line}”`
	);
	await keep('167h-site-confirmations');
	await who.getByRole('button', { name: 'Done' }).click();

	// Taken back, after the core's question.
	await back.click();
	const asks = page.getByRole('alertdialog', { name: 'Take your confirmation of this site back?' });
	await asks.getByRole('button', { name: 'Take my confirmation back' }).click();
	await expect(site.getByRole('status').first()).toHaveText(
		'Your confirmation of this site is taken back.'
	);
	await expect(
		confirmed.getByRole('button', { name: 'I took off here and it was good' })
	).toBeVisible();
	await expect(confirmed).not.toContainText('You confirmed this site');

	expect(problems).toEqual([]);
});

test('a partner’s site offers a pilot no correcting and says why, and still takes a confirmation', async ({
	page,
	mapDrawn,
	keep,
	wide,
	problems
}) => {
	await signUp(page);
	await mapAt(page, mapDrawn, KOGEL);

	// A partner's site is on the map without any switch, and says its tier on its page only.
	const site = await openFromList(page, wide, 'Musterkogel West');
	await expect(site.getByText('From a partner', { exact: true })).toBeVisible();
	const by = site.getByRole('region', { name: 'From a partner' });
	await expect(by.getByRole('button', { name: 'Correct this site' })).toHaveCount(0);
	await expect(by.getByRole('button', { name: 'Remove this site' })).toHaveCount(0);
	await expect(by).toContainText(
		"A partner of Venti added this site, and only Venti's partners change it. If something about it is wrong, report it."
	);
	await expect(by.getByRole('button', { name: 'Report this site' })).toBeVisible();
	await expect(by.getByRole('button', { name: 'History' })).toBeVisible();
	// Any pilot says of it that they took off here.
	await expect(site.getByRole('button', { name: 'I took off here and it was good' })).toBeVisible();
	await by.scrollIntoViewIfNeeded();
	await keep('167i-site-of-a-partner');

	// Its history is read by anyone; putting a revision back is not offered.
	await by.getByRole('button', { name: 'History' }).click();
	const history = page.getByRole('dialog', { name: 'History' });
	const revisions = history.getByRole('list', { name: 'Revisions, the newest first' });
	await expect(revisions.getByRole('listitem').first()).toBeVisible();
	await expect(history.getByRole('button', { name: /Put back|Undo/ })).toHaveCount(0);

	expect(problems).toEqual([]);
});

test('a partner of Venti reads “Add a site”, adds one that every map shows, and corrects a place from pilots', async ({
	page,
	mapDrawn,
	keep,
	wide,
	problems
}) => {
	const name = `Partnerkogel ${Math.random().toString(36).slice(2, 7)}`;
	await signInAsPartner(page);
	await mapAt(page, mapDrawn, somewhere());

	// The partners' entry, and no other.
	await expect(page.getByRole('button', { name: AS_A_PILOT.entry })).toHaveCount(0);
	const form = await addHere(page, wide, AS_A_PARTNER);
	await expect(
		form.getByText(
			"As a partner of Venti you give this site to everyone under CC0 1.0, the public domain: anyone may use it and change it, on Venti and anywhere else. It is shown to every pilot beside the official sites, with your handle; Venti's partners can correct it, and it stays on Venti when your account goes."
		)
	).toBeVisible();
	await expect(
		form.getByText(
			'Added by a partner of Venti and not checked by Venti. A site on the map is not permission to fly there.'
		)
	).toBeVisible();
	await keep('167j-site-form-partner');
	await form.getByRole('textbox', { name: 'Name' }).fill(name);
	await form.getByRole('button', { name: 'Landing' }).click();
	await form.getByRole('textbox', { name: 'Why' }).fill('Made by a test.');
	const before = (await signs(page)).filter((id) => id.startsWith('venti:')).length;
	await form.getByRole('button', { name: AS_A_PARTNER.send }).click();

	// On the map for everyone, with the layer of the pilots' places off.
	const site = panel(page, wide, name);
	await expect(site.getByRole('status').first()).toHaveText(
		'Added. The site is on the map for everyone now.'
	);
	await expect(site.getByText('From a partner', { exact: true })).toBeVisible();
	const all = await signs(page);
	expect(all.filter((id) => id.startsWith('venti:')).length).toBe(before + 1);
	expect(all.filter((id) => id.startsWith('venti-pilots:'))).toEqual([]);
	// A landing is not a place to take off from: no confirming is offered.
	await expect(site.getByRole('button', { name: 'I took off here and it was good' })).toHaveCount(
		0
	);
	await keep('167k-site-added-by-a-partner');

	// Removed again, so that the next run finds the place free.
	const by = site.getByRole('region', { name: 'From a partner' });
	await by.getByRole('button', { name: 'Remove this site' }).click();
	const removing = page.getByRole('alertdialog', { name: 'Remove this site from the map?' });
	await removing.getByRole('button', { name: 'Remove this site' }).click();
	const history = page.getByRole('dialog', { name: 'History' });
	await expect(history.getByRole('button', { name: 'Bring this site back' })).toBeVisible();
	await history.getByRole('button', { name: 'Done' }).click();

	// A partner corrects a place from pilots too: the made-up one, once its layer is on.
	await page.context().setGeolocation(JOCH);
	await page.reload();
	await mapTabDrawn(page, mapDrawn);
	await showMyPosition(page, mapDrawn);
	const filter = await openFilter(page, wide);
	await filter.getByRole('switch', { name: 'Places from pilots' }).check();
	await expect.poll(() => signs(page)).toContain(JOCH_ID);
	await closeFilter(page, wide, filter);
	const joch = await openFromList(page, wide, 'Beispieljoch');
	const fromPilots = joch.getByRole('region', { name: 'From pilots' });
	await fromPilots.getByRole('button', { name: 'Correct this site' }).click();
	const correcting = panel(page, wide, 'Correct this site');
	const fee = `Free, says run ${Math.random().toString(36).slice(2, 7)}`;
	await correcting.getByRole('textbox', { name: 'Fee' }).fill(fee);
	await correcting.getByRole('textbox', { name: 'Why' }).fill('Corrected by a test.');
	const save = correcting.getByRole('button', { name: 'Save for everyone' });
	// The same test runs at other widths on this server and corrects the same
	// place: when another run was quicker, the core carries the entry onto the
	// place as it stands, says so, and the entry is sent again.
	const saved = joch.getByRole('status').filter({ hasText: /^Saved for everyone, as revision/ });
	await expect(async () => {
		if (await correcting.isVisible()) await save.click();
		await expect(saved).toBeVisible({ timeout: 5_000 });
	}).toPass({ timeout: 40_000 });
	await expect(joch).toContainText(fee);
	await expect(fromPilots).toContainText('Last changed by Demo Pilot (@demo)');

	expect(problems).toEqual([]);
});

test('signed out, the entry on the map says to sign in and leads to the account', async ({
	page,
	mapDrawn,
	keep,
	problems
}) => {
	await mapAt(page, mapDrawn, somewhere());

	// The core's sentence in the entry's place, and no entry that opens a form.
	const entry = page.getByRole('link', { name: SIGN_IN_TO_ADD });
	await expect(entry).toBeVisible();
	await expect(page.getByRole('button', { name: AS_A_PILOT.entry })).toHaveCount(0);
	await keep('167m-site-add-signed-out');
	await entry.click();
	await expect(page).toHaveURL(/\/settings\/account\/?$/);
	await expect(page.getByRole('button', { name: 'Sign in' })).toBeVisible();

	expect(problems).toEqual([]);
});

test('a pilot finds a removed place under “Removed lately”, in the core’s line, and brings it back', async ({
	page,
	mapDrawn,
	keep,
	wide,
	problems
}) => {
	// The development server's own removed place is brought back by no test: the
	// same test runs at other widths on this server and would not find it removed.
	// This run removes a place of its own, and brings that one back.
	const name = `Fortkogel ${Math.random().toString(36).slice(2, 7)}`;
	const handle = await signUp(page);
	await mapAt(page, mapDrawn, somewhere());
	const form = await addHere(page, wide);
	await form.getByRole('textbox', { name: 'Name' }).fill(name);
	await form.getByRole('button', { name: 'Take-off' }).click();
	await form.getByRole('button', { name: AS_A_PILOT.send }).click();
	const site = panel(page, wide, name);
	await expect(site.getByRole('status').first()).toHaveText(/^Added\./);
	await site
		.getByRole('region', { name: 'From pilots' })
		.getByRole('button', { name: 'Remove this site' })
		.click();
	const removing = page.getByRole('alertdialog', { name: 'Remove this site from the map?' });
	await removing.getByRole('button', { name: 'Remove this site' }).click();
	const history = page.getByRole('dialog', { name: 'History' });
	await expect(history.getByRole('status').first()).toHaveText(/^Removed from the map\./);
	await history.getByRole('button', { name: 'Done' }).click();

	// The quiet entry at the foot of the filter. With the switch off, no place from pilots is listed.
	const filter = await openFilter(page, wide);
	const entry = filter.getByRole('button', { name: 'Removed lately' });
	await entry.click();
	const removed = page.getByRole('dialog', { name: 'Removed lately' });
	await expect(
		removed.getByRole('list').or(removed.getByText('No site was removed.')).first()
	).toBeVisible();
	await expect(removed.getByRole('listitem').filter({ hasText: name })).toHaveCount(0);
	await expect(removed.getByRole('heading', { name: 'Places from pilots' })).toHaveCount(0);
	await removed.getByRole('button', { name: 'Done' }).click();

	// With the switch on they stand under their layer's name, each in the core's line.
	await filter.getByRole('switch', { name: 'Places from pilots' }).check();
	await entry.click();
	const pilots = removed.getByRole('list', { name: 'Places from pilots' });
	const line = pilots.getByRole('listitem').filter({ hasText: name });
	await expect(line).toContainText(
		new RegExp(`^${name}: removed by Test Pilot \\(@${handle}\\) on \\d+ Nov 2026`)
	);
	// The development server's made-up removed place, which a pilot may bring back too.
	const made = pilots
		.getByRole('listitem')
		.filter({ hasText: /^Musterkogel: removed by Jonas Beispiel \(@jonas\) on \d+ Nov 2026/ });
	await expect(made.getByRole('button', { name: 'Bring this site back' })).toBeVisible();
	await keep('167l-sites-removed-lately');

	// Brought back, after the core's question: its page, and on the map with the switch on.
	await line.getByRole('button', { name: 'Bring this site back' }).click();
	const bringing = page.getByRole('alertdialog', { name: 'Bring this site back?' });
	await expect(bringing).toContainText(
		'It is on the map again as it stood before it was removed, as a new revision.'
	);
	await bringing.getByRole('button', { name: 'Bring this site back' }).click();
	await expect(removed).toBeHidden();
	await expect(site).toBeVisible();
	await expect(site.getByRole('status').first()).toBeVisible();
	await expect
		.poll(async () => (await signs(page)).filter((id) => id.startsWith('venti-pilots:')).length)
		.toBeGreaterThan(0);
	await keep('167n-site-brought-back');
	await site.getByRole('button', { name: wide ? 'Close' : 'Done' }).click();
	await page.getByRole('button', { name: 'Flying sites' }).click();
	const row = page
		.getByRole('dialog', { name: 'Flying sites' })
		.getByRole('listitem')
		.filter({ hasText: name });
	await expect(row).toContainText('From pilots');
	await row.getByRole('button').click();
	await expect(site).toBeVisible();

	// And removed for good, so that the list of the next run is no longer by this one.
	await site
		.getByRole('region', { name: 'From pilots' })
		.getByRole('button', { name: 'Remove this site' })
		.click();
	await removing.getByRole('button', { name: 'Remove this site' }).click();
	await expect(history.getByRole('status').first()).toHaveText(/^Removed from the map\./);

	expect(problems).toEqual([]);
});

/** The requests for who confirmed a site. */
const CONFIRMATIONS = /\/v1\/sites\/[^/]+\/confirmations/;

/**
 * What an open sheet or the page draws wider than the screen, outside a
 * row that scrolls sideways by design: nothing, where the screen fits.
 */
const tooWide = (page: Page) =>
	page.evaluate(() => {
		const scrolls = (element: Element | null): boolean =>
			element !== null &&
			(['auto', 'scroll'].includes(getComputedStyle(element).overflowX) ||
				scrolls(element.parentElement));
		return [...document.querySelectorAll('dialog[open] *, .site *, form *')]
			.filter((element) => {
				const box = element.getBoundingClientRect();
				return box.width > 0 && box.right > innerWidth + 1 && !scrolls(element.parentElement);
			})
			.map((element) => `${element.tagName.toLowerCase()}.${element.className}`);
	});

test('the two tiers fit a narrow phone with large text', async ({
	page,
	mapDrawn,
	keep,
	wide,
	problems
}) => {
	test.skip(wide, 'A narrow phone: 360 px wide, text at 130 %.');
	await page.setViewportSize({ width: 360, height: 780 });
	await signUp(page);
	await mapAt(page, mapDrawn, JOCH);
	await page.evaluate(() => (document.documentElement.style.fontSize = '130%'));

	// The filter's sheet, with the switch and "Removed lately" at its foot.
	const filter = await openFilter(page, false);
	const layer = filter.getByRole('switch', { name: 'Places from pilots' });
	await layer.check();
	await expect.poll(() => signs(page)).toContain(JOCH_ID);
	await filter.getByRole('button', { name: 'Removed lately' }).scrollIntoViewIfNeeded();
	await keep('168a-narrow-filter-switch');
	expect(await tooWide(page)).toEqual([]);
	await closeFilter(page, false, filter);

	// A place from pilots, while the list of who took off there does not arrive.
	let release = () => {};
	const held = new Promise<void>((resolve) => (release = resolve));
	await page.context().route(CONFIRMATIONS, async (route) => {
		await held;
		await route.abort();
	});
	const site = await openFromList(page, false, 'Beispieljoch');
	const confirmed = site.getByRole('region', { name: 'Pilots who took off here' });
	await expect(
		confirmed.getByRole('button', { name: 'I took off here and it was good' })
	).toBeVisible();
	await confirmed.scrollIntoViewIfNeeded();
	await keep('168b-narrow-site-from-pilots');
	expect(await tooWide(page)).toEqual([]);

	// The list: read, not to be had, and there after "Try again".
	await confirmed.getByRole('button', { name: /took off here, last on/ }).click();
	const who = page.getByRole('dialog', { name: 'Pilots who took off here' });
	await expect(who.getByRole('status')).toHaveText('Loading…');
	await keep('168c-narrow-confirmations-loading');
	release();
	await expect(who.getByRole('alert')).toHaveText('Venti could not be reached for the site.');
	await keep('168d-narrow-confirmations-unreachable');
	await page.context().unroute(CONFIRMATIONS);
	await who.getByRole('button', { name: 'Try again' }).click();
	await expect(who.getByRole('listitem').first()).toBeVisible();
	await keep('168e-narrow-confirmations');
	expect(await tooWide(page)).toEqual([]);
	await who.getByRole('button', { name: 'Done' }).click();

	// The small form of "I took off here and it was good".
	await confirmed.getByRole('button', { name: 'I took off here and it was good' }).click();
	const confirming = page.getByRole('dialog', { name: 'I took off here and it was good' });
	await expect(confirming.getByRole('combobox', { name: 'On which day' })).toBeVisible();
	await keep('168f-narrow-confirm-sheet');
	expect(await tooWide(page)).toEqual([]);
	await confirming.getByRole('button', { name: 'Done' }).click();
	await site.getByRole('button', { name: 'Done' }).click();

	// The form for a place from pilots: its head, and its foot with the giving and the button.
	const form = await addHere(page, false);
	await keep('168g-narrow-site-form');
	expect(await tooWide(page)).toEqual([]);
	await form.getByRole('button', { name: AS_A_PILOT.send }).scrollIntoViewIfNeeded();
	await keep('168h-narrow-site-form-giving');
	expect(await tooWide(page)).toEqual([]);

	// The request that was let fail is the only thing the console complains of.
	expect(problems.filter((problem) => !CONFIRMATIONS.test(problem))).toEqual([]);
});
