/**
 * Groups as communities with a chat (owner decision 26): finding public
 * groups near a place or by name, joining one, its chat with a flight's
 * card, posting a message (the website's own addresses of a site and an
 * open plan become cards), who sees a plan in a public group, and making
 * a private group, whose page asks the server for it no second time, with
 * an invite by handle. The server is the development mode's, with its six made-up
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

	// The website's own addresses of a site and of an open plan, pasted, become cards.
	const origin = new URL(page.url()).origin;
	await field.fill(
		`The take-off: ${origin}/places/47.28000,10.90000/?site=venti%3A0000000000000000000000s001 ` +
			`and the lift: ${origin}/plans/open/?id=aaaaaaaaaaaaaaaaaaaaaaaaaa`
	);
	await page.getByRole('button', { name: 'Send' }).click();
	const pasted = messages.getByRole('listitem').last();
	await expect(pasted.getByRole('link', { name: 'Musterkogel West' })).toHaveAttribute(
		'href',
		/\/places\/[\d.]+,[\d.]+\/\?site=venti%3A0000000000000000000000s001$/
	);
	await expect(pasted.locator('.card.open_plan a')).toHaveAttribute(
		'href',
		/\/plans\/open\/?\?id=aaaaaaaaaaaaaaaaaaaaaaaaaa$/
	);
	await keep('303-group-chat');

	// A plan in a public group: the form says that anyone sees it, in the core's words.
	await page.getByRole('button', { name: 'New plan' }).click();
	const planForm = page.getByRole('dialog', { name: 'New plan' });
	await expect(planForm).toContainText(
		'The group is public: anyone can see the plan. It is removed two days after its day.'
	);
	await planForm.getByRole('button', { name: 'Done' }).click();
	await expect(planForm).toBeHidden();

	// The group is now among the pilot's own, with its last message.
	await page.getByRole('main').getByRole('link', { name: 'Community', exact: true }).click();
	const mine = page.getByRole('list', { name: 'Your groups' });
	await expect(mine.getByRole('listitem', { name: 'Rosenheimer Eagles' })).toContainText(
		'The take-off:'
	);
	// Under the plans: who sees them, now that one of the pilot's groups is public.
	await expect(
		page.getByText(
			'A plan in a public group is seen by anyone, one in a private group by its members and nobody else. A plan is removed two days after its day.'
		)
	).toBeVisible();
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
	// The map of the home, whole and opened on it, once drawn.
	await expect(form.locator('[aria-busy="false"]')).toBeVisible();
	await keep('305-new-group');
	// The answer that made the group is the group: its page asks for it no more.
	const askedForGroup: string[] = [];
	page.on('request', (request) => {
		const path = new URL(request.url()).pathname;
		if (request.method() === 'GET' && /^\/v1\/groups\/[0-9a-z]{26}$/.test(path)) {
			askedForGroup.push(path);
		}
	});
	await form.getByRole('button', { name: 'Make the group' }).click();

	await expect(page.getByRole('heading', { level: 1, name: 'Hausberg-Runde' })).toBeVisible();
	await expect(page.getByText('Private ·')).toBeVisible();
	await expect(page.getByText('No messages yet.')).toBeVisible();
	expect(askedForGroup).toEqual([]);

	// The owner invites a pilot by handle, from the settings.
	await page.getByRole('button', { name: 'Settings' }).click();
	const settings = page.getByRole('dialog', { name: 'Group settings' });
	await settings.getByRole('textbox', { name: 'Invite' }).fill('@mara');
	await settings.getByRole('button', { name: 'Invite' }).click();
	await expect(settings.getByRole('status')).toHaveText('Invited to Hausberg-Runde.');
	await keep('306-group-settings');
	expect(problems).toEqual([]);
});
