/**
 * Alarms on the account: the list under the account with what fired, and
 * a wind alarm made from a station's page. The development server's demo
 * account has a wind alarm, "Patscherkofel north", that fired once, and a
 * day alarm, "Nordkette cross-country"; it is only read. Tests run side by
 * side on one server, so the one that makes an alarm makes an account of
 * its own.
 */

import type { Page } from '@playwright/test';
import { expect, mapTabDrawn, metric, test } from './helpers';

const DEMO = { handle: 'demo', password: 'demo-pilot-password' };

const field = (page: Page, name: string) => page.getByRole('textbox', { name, exact: true });

async function signIn(page: Page): Promise<void> {
	await page.goto('/settings/account/');
	await field(page, 'Handle').fill(DEMO.handle);
	await field(page, 'Password').fill(DEMO.password);
	await page.getByRole('button', { name: 'Sign in' }).click();
	await expect(metric(page, 'Handle')).toHaveText(`@${DEMO.handle}`);
}

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

test('the account’s alarms are listed with what they fired', async ({ page, keep, problems }) => {
	await signIn(page);
	await page.getByRole('link', { name: /^Alerts/ }).click();
	await expect(page.getByRole('heading', { name: 'Alerts', level: 1 })).toBeVisible();

	const list = page.getByRole('list', { name: 'Your alarms' });
	await expect(list.getByRole('listitem')).toHaveCount(2);
	await expect(list).toContainText('Patscherkofel north');
	await expect(list).toContainText(
		'Wind at Patscherkofel from N to NE, 10 to 20 km/h, gusts at most 30, between 10:00 and 16:00'
	);
	await expect(list).toContainText('Nordkette cross-country');
	await expect(list.getByRole('switch', { name: 'Patscherkofel north on' })).toBeChecked();

	const fired = page.getByRole('list', { name: 'What fired' });
	await expect(fired.getByRole('listitem')).toHaveCount(1);
	await expect(fired).toContainText(
		/^Patscherkofel: 15 km\/h from the north, gusts 22, at \d\d:20, as your alarm 'Patscherkofel north' asks/
	);
	await keep('240-alerts');

	// The same in the dark.
	await page.emulateMedia({ colorScheme: 'dark' });
	await keep('241-alerts-dark');
	await page.emulateMedia({ colorScheme: 'light' });

	// A tap opens the alarm in its form, as the core turns it back into a draft.
	await list.getByRole('button', { name: /^Patscherkofel north/ }).click();
	const form = page.getByRole('dialog', { name: 'Change the alarm' });
	await expect(form.locator('[data-alert-line]')).toHaveText(
		'Wind at Patscherkofel from N to NE, 10 to 20 km/h, gusts at most 30, between 10:00 and 16:00'
	);
	await expect(form.getByRole('button', { name: 'NE', exact: true }).last()).toHaveAttribute(
		'aria-pressed',
		'true'
	);
	await form.getByRole('button', { name: 'Done' }).click();
	expect(problems).toEqual([]);
});

test('a wind alarm is made from a station’s page, its fault shown and put right', async ({
	page,
	mapDrawn,
	keep,
	wide,
	problems
}) => {
	await signUp(page);
	await page.goto('/map/');
	await mapTabDrawn(page, mapDrawn);
	await page.getByRole('button', { name: 'Wind stations' }).click();
	const stations = page.getByRole('dialog', { name: 'Wind stations' });
	await stations.getByRole('searchbox', { name: 'Station' }).fill('Patscherkofel');
	await stations.getByRole('listitem').first().getByRole('button').click();
	const station = page.getByRole(wide ? 'complementary' : 'dialog', { name: 'Patscherkofel' });
	await station.getByRole('button', { name: 'Alert me' }).click();

	const form = page.getByRole('dialog', { name: 'Alert me' });
	await expect(form.getByRole('textbox', { name: 'Name', exact: true })).toHaveValue(
		'Patscherkofel'
	);
	// Nothing chosen yet: the core says what is missing, at the directions.
	await form.getByRole('button', { name: 'Save' }).click();
	const fault = form.getByRole('alert').filter({ hasText: /^The directions: / });
	await expect(fault).toBeVisible();
	await keep('242-alert-form-fault');

	// A least wind puts it right, and the core's line says what it waits for.
	await form.getByRole('textbox', { name: 'Least wind', exact: true }).fill('12');
	await expect(fault).toBeHidden();
	await expect(form.locator('[data-alert-line]')).toHaveText(
		'Wind at Patscherkofel, at least 12 km/h'
	);
	await form.getByRole('button', { name: 'N', exact: true }).first().click();
	await expect(form.locator('[data-alert-line]')).toHaveText(
		'Wind at Patscherkofel from N, at least 12 km/h'
	);
	await keep('243-alert-form');
	await page.emulateMedia({ colorScheme: 'dark' });
	await keep('244-alert-form-dark');
	await page.emulateMedia({ colorScheme: 'light' });
	await form.getByRole('button', { name: 'Save' }).click();
	await expect(form).toBeHidden();

	await page.goto('/settings/account/alerts/');
	const list = page.getByRole('list', { name: 'Your alarms' });
	await expect(list.getByRole('listitem')).toHaveCount(1);
	await expect(list).toContainText('Wind at Patscherkofel from N, at least 12 km/h');
	await expect(page.getByText('Nothing has fired in the last 30 days.')).toBeVisible();
	await keep('245-alerts-new');
	expect(problems).toEqual([]);
});
