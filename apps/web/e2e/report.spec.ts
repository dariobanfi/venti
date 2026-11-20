/**
 * Reporting what another pilot shares to whoever runs the server: a
 * flight, a pilot's page, a live track. The server is the development
 * mode's, which takes reports and keeps them in memory.
 *
 * A report is sent from an account. Every test that sends one makes an
 * account of its own, so that tests running side by side, and again on the
 * same server, do not meet each other's reports.
 */

import type { Locator, Page } from '@playwright/test';
import { expect, mapTabDrawn, metric, showMyPosition, test } from './helpers';

/** The newest of the development mode's public flights, Jonas Beispiel's. */
const FLIGHT = 'nfmvgygyr34epnfbcekb04nwqc';

/** Between the development mode's two gliders, for the pilot to stand at. */
const BETWEEN = { latitude: 47.678, longitude: 11.755 };

const NOTICE =
	'Whoever runs this server reads the report and sees that it came from you. The pilot you report is not told, and no answer comes back.';

const field = (page: Page, name: string) => page.getByRole('textbox', { name });

/** Creates an account nobody has used on this server, and leaves the browser signed in to it. */
async function signUp(page: Page): Promise<void> {
	const handle = `pilot${Math.random().toString(36).slice(2, 10)}`;
	await page.goto('/settings/account/');
	await page.getByRole('radio', { name: 'New account' }).check();
	await field(page, 'Handle').fill(handle);
	await field(page, 'Name shown to others').fill('Test Pilot');
	await field(page, 'Password').fill('correct horse battery staple');
	await page.getByRole('button', { name: 'Create account' }).click();
	await expect(metric(page, 'Handle')).toHaveText(`@${handle}`);
}

/** Opens the report form of another pilot's flight from the flight's menu. */
async function reportTheFlight(page: Page): Promise<Locator> {
	await page.goto(`/logbook/everyone/flight/?id=${FLIGHT}`);
	await page.getByRole('button', { name: 'More' }).click();
	await page.getByRole('menuitem', { name: 'Report this flight' }).click();
	const form = page.getByRole('dialog', { name: 'Report this flight' });
	await expect(form).toBeVisible();
	return form;
}

test('a pilot reports another pilot’s flight, and is told beforehand what happens to a report', async ({
	page,
	keep,
	problems
}) => {
	await signUp(page);
	const form = await reportTheFlight(page);
	const send = form.getByRole('button', { name: 'Send report' });

	// The server's six reasons, each with a line that says what it means.
	await expect(form.getByRole('radio')).toHaveCount(6);
	await expect(form.getByRole('radio', { name: 'Not a real flight' })).toBeVisible();
	await expect(form).toContainText(
		'A track that is made up or altered, or that this pilot did not fly.'
	);
	// What happens to a report stands under the field, before anything is sent.
	await expect(form.getByText(NOTICE)).toBeVisible();
	// Nothing can be sent before a reason is chosen.
	await expect(send).toBeDisabled();

	// "Something else" has to say what: the core refuses it before the
	// server is asked.
	await form.getByRole('radio', { name: 'Something else' }).check();
	await expect(field(page, 'What is it?')).toBeVisible();
	await send.click();
	await expect(form.getByRole('alert')).toHaveText('Say in a few words what is wrong.');

	// A reason of the list, with a few words of the pilot's own.
	await form.getByRole('radio', { name: 'Not a real flight' }).check();
	// What was wrong with the report before is not said about this one.
	await expect(form.getByRole('alert')).toBeHidden();
	await field(page, 'Anything to add?').fill('I flew there that day. This is my track.');
	await keep('67-report-form');
	await send.click();

	// Sent: one line, and nothing more to fill in.
	await expect(form.getByRole('status')).toHaveText('Sent. Whoever runs this server will read it.');
	await expect(form.getByRole('radio')).toHaveCount(0);
	await expect(send).toBeHidden();
	await keep('68-report-sent');
	await form.getByRole('button', { name: 'Done' }).click();
	await expect(form).toBeHidden();

	// Reported again, the server stores nothing new, and the form says so.
	await page.getByRole('button', { name: 'More' }).click();
	await page.getByRole('menuitem', { name: 'Report this flight' }).click();
	// The form starts empty every time it opens.
	await expect(send).toBeDisabled();
	await expect(field(page, 'Anything to add?')).toHaveValue('');
	await form.getByRole('radio', { name: 'Advertising' }).check();
	await send.click();
	await expect(form.getByRole('status')).toHaveText(
		'You reported this before. Your report is still waiting to be read.'
	);
	expect(problems).toEqual([]);
});

test('a report needs an account: signed out, the form says so and offers nothing to fill in', async ({
	page,
	keep
}) => {
	const form = await reportTheFlight(page);

	await expect(form).toContainText(
		'A report is sent from your account, so that Venti knows who it comes from. Sign in under Settings, Account.'
	);
	await expect(form.getByRole('radio')).toHaveCount(0);
	await expect(form.getByRole('textbox')).toHaveCount(0);
	await expect(form.getByRole('button', { name: 'Send report' })).toBeHidden();
	await keep('69-report-signed-out');
	// The way to an account is one click.
	await form.getByRole('link', { name: 'Account' }).click();
	await expect(page.getByRole('heading', { level: 1, name: 'Account' })).toBeVisible();
});

test('a pilot’s page is reported from the pilot’s flights', async ({ page }) => {
	await signUp(page);
	await page.goto('/logbook/everyone/pilot/?handle=mara');
	await expect(page.getByRole('heading', { level: 1, name: 'Mara Muster' })).toBeVisible();

	await page.getByRole('button', { name: 'Report this pilot' }).click();
	const form = page.getByRole('dialog', { name: 'Report this pilot' });
	await expect(form.getByText(NOTICE)).toBeVisible();
	await form.getByRole('radio', { name: 'Pretends to be someone else' }).check();
	await form.getByRole('button', { name: 'Send report' }).click();
	await expect(form.getByRole('status')).toHaveText('Sent. Whoever runs this server will read it.');
});

test('a live track is reported from its glider’s page on the map', async ({
	context,
	page,
	mapDrawn,
	keep,
	wide
}) => {
	await signUp(page);
	await context.grantPermissions(['geolocation']);
	await context.setGeolocation(BETWEEN);
	await page.goto('/');
	await mapTabDrawn(page, mapDrawn);
	await showMyPosition(page, mapDrawn);
	await page.getByRole('button', { name: 'In the air' }).click();
	await page
		.getByRole('dialog', { name: 'In the air' })
		.getByRole('listitem')
		.first()
		.getByRole('button')
		.click();

	const glider = page.getByRole(wide ? 'complementary' : 'dialog', { name: '@demo' });
	await glider.getByRole('button', { name: 'Report this track' }).click();
	const form = page.getByRole('dialog', { name: 'Report this track' });
	await expect(form.getByRole('radio')).toHaveCount(6);
	await form.getByRole('radio', { name: 'Shows something private' }).check();
	await keep('70-report-track');
	await form.getByRole('button', { name: 'Send report' }).click();
	await expect(form.getByRole('status')).toHaveText('Sent. Whoever runs this server will read it.');
	// The glider's page is still there behind the form.
	await form.getByRole('button', { name: 'Done' }).click();
	await expect(glider).toBeVisible();
});
