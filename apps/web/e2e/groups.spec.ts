/**
 * Groups as communities with a chat (owner decision 26): finding public
 * groups near a place or by name, joining one, its chat with a flight's
 * card, posting a message, and making a private group with an invite by
 * handle. The server is the development mode's, with its six made-up
 * public groups around Munich and the mountains south of it.
 *
 * Every test makes an account of its own, so that tests running side by
 * side, and again on the same server, do not meet each other's groups.
 */

import type { Page } from '@playwright/test';
import { expect, signUp, test } from './helpers';

/** Community's groups, from the navigation. */
async function openCommunity(page: Page): Promise<void> {
	await page
		.getByRole('navigation', { name: 'Sections' })
		.getByRole('link', { name: /^Community/ })
		.click();
	await expect(page.getByRole('heading', { level: 1, name: 'Community' })).toBeVisible();
}

test('a pilot finds the Rosenheimer Eagles, joins them and writes in their chat', async ({
	page,
	keep,
	problems
}) => {
	await signUp(page);
	await openCommunity(page);

	// In no group yet; the groups near are listed, and a search by place finds more.
	await expect(page.getByText('Join a group near you, or make one.')).toBeVisible();
	await expect(
		page.getByRole('list', { name: 'Groups near you' }).getByRole('listitem').first()
	).toBeVisible();
	await page.getByRole('searchbox', { name: 'Search by name or place' }).fill('Rosenheim');
	const found = page.getByRole('list', { name: 'Groups found' });
	const eagles = found.getByRole('listitem', { name: 'Rosenheimer Eagles' });
	await expect(eagles).toContainText('Rosenheim and the Chiemgau');
	await keep('302-groups-found');

	// Joining opens the group's page, with its chat.
	await eagles.getByRole('button', { name: 'Join' }).click();
	await expect(page).toHaveURL(/\/community\/groups\/0000000000000000000000g002\/$/);
	await expect(page.getByRole('heading', { level: 1, name: 'Rosenheimer Eagles' })).toBeVisible();
	const messages = page.getByRole('list', { name: 'Messages' });
	await expect(messages).toContainText('My flight from yesterday, the inversion broke late.');

	// Mara's flight is a card with its small track, and leads to the flight.
	const card = messages.getByRole('link', { name: 'Flight by Mara Muster' });
	await expect(card).toBeVisible();
	await expect(messages.locator('.card.flight svg polyline').first()).toBeAttached();

	// A message of the pilot's own, at the bottom.
	const field = page.getByRole('textbox', { name: 'Message' });
	await field.fill('Going up at noon, anyone?');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(field).toHaveValue('');
	await expect(messages.getByRole('listitem').last()).toContainText('Going up at noon, anyone?');
	await expect(messages.getByRole('listitem').last()).toContainText('You');
	await keep('303-group-chat');

	// The group is now among the pilot's own, with its last message.
	await page.getByRole('main').getByRole('link', { name: 'Community', exact: true }).click();
	const mine = page.getByRole('list', { name: 'Your groups' });
	await expect(mine.getByRole('listitem', { name: 'Rosenheimer Eagles' })).toContainText(
		'Going up at noon, anyone?'
	);
	await keep('304-your-groups');

	// The card opens the flight.
	await mine.getByRole('link', { name: 'Rosenheimer Eagles' }).click();
	await card.click();
	await expect(page).toHaveURL(/\/flights\/[0-9a-z]+\/$/);
	expect(problems).toEqual([]);
});

test('a pilot makes a private group and invites a pilot by handle', async ({
	page,
	keep,
	problems
}) => {
	await signUp(page);
	await openCommunity(page);

	await page.getByRole('button', { name: 'New group' }).click();
	const form = page.getByRole('dialog', { name: 'New group' });
	await form.getByRole('textbox', { name: 'Name' }).fill('Hausberg-Runde');
	await form.getByRole('textbox', { name: 'Description' }).fill('Who goes up after work.');
	await form.getByRole('radio', { name: 'Private' }).check();
	await form.getByRole('textbox', { name: 'Place' }).fill('Musterberg');
	await keep('305-new-group');
	await form.getByRole('button', { name: 'Make the group' }).click();

	await expect(page.getByRole('heading', { level: 1, name: 'Hausberg-Runde' })).toBeVisible();
	await expect(page.getByText('Private ·')).toBeVisible();
	await expect(page.getByText('No messages yet.')).toBeVisible();

	// The owner invites a pilot by handle, from the settings.
	await page.getByRole('button', { name: 'Settings' }).click();
	const settings = page.getByRole('dialog', { name: 'Group settings' });
	await settings.getByRole('textbox', { name: 'Invite' }).fill('@mara');
	await settings.getByRole('button', { name: 'Invite' }).click();
	await expect(settings.getByRole('status')).toHaveText('Invited to Hausberg-Runde.');
	await keep('306-group-settings');
	expect(problems).toEqual([]);
});
