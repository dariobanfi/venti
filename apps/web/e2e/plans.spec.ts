/**
 * Flying plans: the board of who means to fly where, the first part of
 * Community, a plan with who is in, the form for a plan, and the groups,
 * with what a keeper and a member can do. The server is the development mode's, which keeps groups and
 * plans in memory and takes any registration.
 *
 * Every test makes accounts of its own, and the long one a group of its
 * own, so that tests running side by side, and again on the same server,
 * do not meet each other's plans. The development mode's made-up plans are
 * for 18 to 20 November 2026, and its clock stands still; a browser's does
 * not. So nothing here looks for those plans: a test posts one for its own
 * today and looks for that.
 */

import type { Locator, Page } from '@playwright/test';
import { expect, metric, test, withPasswordForm } from './helpers';

/** The development mode's private group that the demo pilot keeps. */
const MUSTERGRUPPE = {
	id: '44444444444444444444444444',
	name: 'Mustergruppe'
};

const PASSWORD = 'correct horse battery staple';

const NEEDS_ACCOUNT =
	'Plans for a flying day are shared in groups on Venti. Sign in under Settings, Account, then make a group or join one.';

const field = (page: Page, name: string) => page.getByRole('textbox', { name });

/** The navigation, where "Community" is a section beside the map and the flights. */
const sections = (page: Page) => page.getByRole('navigation', { name: 'Sections' });

/** Creates an account nobody has used on this server, and leaves the browser signed in to it. */
async function signUp(page: Page, name: string): Promise<string> {
	const handle = `pilot${Math.random().toString(36).slice(2, 10)}`;
	await page.goto('/settings/account/');
	await page.getByRole('radio', { name: 'New account' }).check();
	await field(page, 'Handle').fill(handle);
	await field(page, 'Name shown to others').fill(name);
	await field(page, 'Password').fill(PASSWORD);
	await page.getByRole('button', { name: 'Create account' }).click();
	await expect(metric(page, 'Handle')).toHaveText(`@${handle}`);
	return handle;
}

/** Opens the board, Community's first part, from wherever the pilot is, without loading the site anew. */
async function openBoard(page: Page): Promise<void> {
	await sections(page)
		.getByRole('link', { name: /^Community/ })
		.click();
	await expect(page.getByRole('heading', { level: 1, name: 'Community' })).toBeVisible();
}

/** The way back from a plan or the groups to Community, in the page's head. */
const backToTheBoard = (page: Page): Locator =>
	page.getByRole('main').getByRole('link', { name: 'Community', exact: true });

/** A day by the browser's own clock, as a date field writes it: today, or some days on. */
function dayByTheBrowser(page: Page, daysOn = 0): Promise<string> {
	return page.evaluate((daysOn) => {
		const day = new Date();
		day.setDate(day.getDate() + daysOn);
		const two = (value: number) => String(value).padStart(2, '0');
		return `${day.getFullYear()}-${two(day.getMonth() + 1)}-${two(day.getDate())}`;
	}, daysOn);
}

/** The plans of the board on a day, by what the day is called: each plan is a card that names its day. */
const plansOf = (page: Page, day: string): Locator =>
	page
		.getByRole('list', { name: 'Plans', exact: true })
		.locator('> li')
		.filter({ has: page.locator('.day').getByText(day, { exact: true }) });

/** The pilot's word on a plan of the board, a button that stays pressed while the pilot is in. */
const imIn = (plan: Locator): Locator => plan.getByRole('button', { name: 'I am in' });

/** Fills the form for a plan and saves it. */
async function savePlan(
	page: Page,
	title: 'New plan' | 'Change the plan',
	plan: { where: string; day?: string; time?: string; note?: string }
): Promise<void> {
	const form = page.getByRole('dialog', { name: title });
	await field(page, 'Where').fill(plan.where);
	if (plan.day) await form.getByLabel('Day').fill(plan.day);
	if (plan.time) {
		await form.getByRole('switch', { name: 'Set a time' }).check();
		await form.getByLabel('Time', { exact: true }).fill(plan.time);
	}
	if (plan.note) await field(page, 'Note').fill(plan.note);
	await form.getByRole('button', { name: 'Save' }).click();
	await expect(form).toBeHidden();
}

/**
 * Posts a plan the way a phone does when its place is taken from the
 * pilot's waypoints: with where that is. A browser has no waypoints, so
 * the web app's own form gives a plan a name for its place and no more.
 */
async function postFromAPhone(
	page: Page,
	handle: string,
	plan: { day: string; place: string; position: { lat: number; lon: number } }
): Promise<void> {
	const signedIn = await page.request.post('/v1/sessions', {
		data: { handle, password: PASSWORD, device: 'A phone' }
	});
	const { token } = await signedIn.json();
	const posted = await page.request.post(`/v1/groups/${MUSTERGRUPPE.id}/plans`, {
		headers: { authorization: `Bearer ${token}` },
		data: plan
	});
	expect(posted.status()).toBe(201);
}

/** Signs a pilot in from the side, as a phone would, for what the browser's own pilot does not do. */
async function tokenOf(page: Page, handle: string, password = PASSWORD): Promise<string> {
	const signedIn = await page.request.post('/v1/sessions', {
		data: { handle, password, device: 'A phone' }
	});
	return (await signedIn.json()).token;
}

/**
 * The demo pilot, who keeps the Mustergruppe, invites the pilot by handle
 * from a phone; the pilot accepts on Community and is on the group's page.
 */
async function intoMustergruppe(page: Page, handle: string): Promise<void> {
	const token = await tokenOf(page, 'demo', 'demo-pilot-password');
	const invited = await page.request.post(`/v1/groups/${MUSTERGRUPPE.id}/invites`, {
		headers: { authorization: `Bearer ${token}` },
		data: { handle }
	});
	expect(invited.status()).toBe(201);
	await page.goto('/community/');
	await accept(page, MUSTERGRUPPE.name);
}

/** Joins a group the pilot was invited into on Community, which opens the group's page. */
async function accept(page: Page, name: string): Promise<void> {
	await page
		.getByRole('list', { name: 'Invites' })
		.getByRole('listitem')
		.filter({ hasText: name })
		.getByRole('button', { name: 'Join' })
		.click();
	await expect(page.getByRole('heading', { level: 1, name })).toBeVisible();
}

/** Opens a group's settings from its page. */
async function settingsOf(page: Page): Promise<Locator> {
	await page.getByRole('button', { name: 'About the group', exact: true }).click();
	return page.getByRole('dialog', { name: 'About the group' });
}

/** Leaves the group whose page is open, from its settings. */
async function leaveTheGroup(page: Page): Promise<void> {
	const settings = await settingsOf(page);
	await settings.getByRole('button', { name: 'Leave', exact: true }).click();
	await page
		.getByRole('alertdialog', { name: 'Leave this group?' })
		.getByRole('button', { name: 'Leave the group' })
		.click();
	await expect(page.getByRole('heading', { level: 1, name: 'Community' })).toBeVisible();
}

test('signed out, Community finds a pilot and asks to sign in for the rest', async ({
	page,
	keep,
	problems
}) => {
	await page.goto('/map/');
	await openBoard(page);
	await expect(page).toHaveTitle('Community · Venti');

	// Nothing to plan with, nobody followed, nothing told.
	await expect(page.getByText(/^With an account you plan flying days/)).toBeVisible();
	await expect(page.getByRole('main').getByRole('link', { name: 'Sign in' })).toBeVisible();
	await expect(page.getByRole('button', { name: 'New plan' })).toBeHidden();
	await expect(page.getByRole('link', { name: 'Groups' })).toBeHidden();
	await expect(page.getByRole('region', { name: 'Following' })).toHaveCount(0);
	await expect(page.getByRole('region', { name: 'Notifications' })).toHaveCount(0);

	// Finding a pilot is for everyone.
	const find = page.getByRole('region', { name: 'Find a pilot' });
	await find.getByRole('searchbox', { name: 'Find a pilot' }).fill('le');
	const found = find.getByRole('list', { name: 'Pilots found' }).getByRole('link');
	await expect(found.first()).toHaveAccessibleName('Lena Probe (@lena), 3 public flights');
	await keep('74-plans-signed-out');
	await found.first().click();
	await expect(page.getByRole('heading', { level: 1, name: 'Lena Probe' })).toBeVisible();

	// The board's old address leads to Community.
	await page.goto('/plans/');
	await expect(page).toHaveURL(/\/community\/$/);
	await expect(page.getByRole('heading', { level: 1, name: 'Community' })).toBeVisible();

	// The groups' old address leads to Community too; a plan says the same.
	await page.goto('/plans/groups/');
	await expect(page).toHaveURL(/\/community\/#groups$/);
	await page.goto('/plans/plan/?id=66666666666666666666666666');
	await expect(page.getByText(NEEDS_ACCOUNT)).toBeVisible();

	// The way to an account is one click.
	await page.getByRole('main').getByRole('link', { name: 'Account' }).click();
	await expect(page.getByRole('heading', { level: 1, name: 'Account' })).toBeVisible();
	expect(problems).toEqual([]);
});

test('a pilot makes a private group, invites a pilot by handle, and plans a flying day with who comes in', async ({
	browser,
	page,
	keep,
	problems
}) => {
	await signUp(page, 'Kira Pilot');

	// Without a group there is nobody to plan with, and the board says so.
	await openBoard(page);
	await expect(page.getByText('You are in no group yet. In a group')).toBeVisible();
	await expect(page.getByRole('button', { name: 'New plan' })).toBeHidden();
	await keep('75-plans-no-group');

	// Making one asks for its name. A name the server would refuse, the
	// core refuses before it is asked.
	await page.getByRole('button', { name: 'New group' }).click();
	const naming = page.getByRole('dialog', { name: 'New group' });
	await expect(naming.getByRole('button', { name: 'Save' })).toBeDisabled();
	await naming.getByRole('textbox', { name: 'Name' }).fill('x'.repeat(61));
	await naming.getByRole('button', { name: 'Save' }).click();
	await expect(naming.getByRole('alert')).toHaveText("A group's name has room for 60 characters.");
	await naming.getByRole('textbox', { name: 'Name' }).fill('Hausberg-Runde');
	await naming.getByRole('radio', { name: 'Private' }).check();
	await naming.getByRole('button', { name: 'Save' }).click();
	await expect(page.getByRole('heading', { level: 1, name: 'Hausberg-Runde' })).toBeVisible();
	const groupPage = new URL(page.url()).pathname;

	// Another pilot, in a browser of their own, is invited by handle and accepts.
	const others = await browser.newContext();
	await withPasswordForm(others);
	const other = await others.newPage();
	const mira = await signUp(other, 'Mira Member');
	const settings = await settingsOf(page);
	await settings.getByRole('textbox', { name: 'Invite by handle' }).fill(`@${mira}`);
	await settings.getByRole('button', { name: 'Invite' }).click();
	await expect(settings.getByRole('status')).toHaveText('Invited to Hausberg-Runde.');
	await keep('76-group-invite');
	await settings.getByRole('button', { name: 'Done' }).click();
	await other.goto('/community/');
	await expect(other.getByRole('list', { name: 'Invites' })).toContainText('Hausberg-Runde');
	await accept(other, 'Hausberg-Runde');
	await expect(other.getByText(/2 members/)).toBeVisible();

	// She posts where she means to fly today.
	await openBoard(other);
	await other.getByRole('button', { name: 'New plan' }).click();
	await savePlan(other, 'New plan', { where: 'Beispielwiese', time: '09:00' });
	await expect(plansOf(other, 'Today')).toContainText(['Beispielwiese']);

	// The keeper's board is asked for from the browser's own today on, and
	// has her plan under "Today".
	const today = await dayByTheBrowser(page);
	const boardAsked = page.waitForRequest((request) => request.url().includes('/v1/groups/plans?'));
	await openBoard(page);
	expect(new URL((await boardAsked).url()).search).toBe(`?from=${today}`);
	await expect(plansOf(page, 'Today')).toHaveCount(1);
	await expect(plansOf(page, 'Today').first()).toContainText('Beispielwiese');
	await expect(plansOf(page, 'Today').first()).toContainText('09:00');
	await expect(plansOf(page, 'Today').first()).toContainText('Mira Member');

	// A plan of his own: a short form, which says who will see it. Without
	// a place nothing can be saved.
	await page.getByRole('button', { name: 'New plan' }).click();
	const form = page.getByRole('dialog', { name: 'New plan' });
	await expect(form.getByRole('button', { name: 'Save' })).toBeDisabled();
	await expect(form.getByLabel('Day')).toHaveValue(today);
	await expect(form).toContainText(
		'The members of the group see the plan, and nobody else. It is removed two days after its day.'
	);
	// With one group there is nothing to choose.
	await expect(form.getByLabel('Group')).toBeHidden();
	await field(page, 'Where').fill('Musterberg Nord');
	await form.getByRole('switch', { name: 'Set a time' }).check();
	await form.getByLabel('Time', { exact: true }).fill('10:30');
	await field(page, 'Note').fill('At the lift at half past ten.\nIn the air by noon.');
	await keep('77-plan-form');
	await form.getByRole('button', { name: 'Save' }).click();
	await expect(form).toBeHidden();

	// And one for tomorrow, without a time.
	await page.getByRole('button', { name: 'New plan' }).click();
	await savePlan(page, 'New plan', {
		where: 'Musterberg, Startplatz Ost',
		day: await dayByTheBrowser(page, 1)
	});

	// The board: day by day, within a day by time, each plan with who is
	// in and its group, and a tick where the pilot is in.
	await expect(plansOf(page, 'Today')).toContainText(['Beispielwiese', 'Musterberg Nord']);
	const mine = plansOf(page, 'Today').filter({ hasText: 'Musterberg Nord' });
	await expect(mine).toContainText('Only you so far');
	await expect(mine).toContainText('Hausberg-Runde');
	await expect(mine).toContainText('10:30');
	await expect(imIn(mine)).toHaveAttribute('aria-pressed', 'true');
	// Who is in as faces: Kira Pilot's initials, the pilot's own.
	const faces = mine.getByRole('list', { name: 'Who is in' }).getByRole('listitem');
	await expect(faces).toHaveCount(1);
	await expect(faces).toContainText('KP');
	await expect(imIn(plansOf(page, 'Today').filter({ hasText: 'Beispielwiese' }))).toHaveAttribute(
		'aria-pressed',
		'false'
	);
	await expect(plansOf(page, 'Tomorrow')).toContainText(['Musterberg, Startplatz Ost']);
	await expect(
		page.getByText(
			'A plan is seen by the members of its group and nobody else, and is removed two days after its day.'
		)
	).toBeVisible();
	await keep('78-plans-board');

	// The plan itself: everything it says, who is in, and the pilot's word.
	await mine.getByRole('link').click();
	await expect(page).toHaveTitle('Plan · Venti');
	await expect(metric(page, 'Where')).toHaveText('Musterberg Nord');
	await expect(metric(page, 'Day')).toHaveText('Today');
	await expect(metric(page, 'Time')).toHaveText('10:30');
	await expect(metric(page, 'Group')).toHaveText('Hausberg-Runde');
	await expect(metric(page, 'Posted by')).toHaveText('You');
	await expect(page.getByText('At the lift at half past ten.')).toBeVisible();
	const whoIsIn = page.getByRole('list', { name: 'Who is in' }).getByRole('listitem');
	// Whoever posts a plan is in from the start, and can step out again.
	await expect(whoIsIn).toHaveText(['You']);
	// One's own is not reported.
	await expect(page.getByRole('button', { name: 'Report this plan' })).toBeHidden();
	await keep('79-plan');
	await page.getByRole('button', { name: 'I am out' }).click();
	await expect(whoIsIn).toHaveText(['Nobody yet']);
	await page.getByRole('button', { name: 'I am in' }).click();
	await expect(whoIsIn).toHaveText(['You']);

	// Its author changes it: the form starts from the plan as it is.
	await page.getByRole('button', { name: 'Change the plan' }).click();
	const changing = page.getByRole('dialog', { name: 'Change the plan' });
	await expect(field(page, 'Where')).toHaveValue('Musterberg Nord');
	await expect(changing.getByLabel('Time', { exact: true })).toHaveValue('10:30');
	await savePlan(page, 'Change the plan', { where: 'Musterberg Süd' });
	await expect(metric(page, 'Where')).toHaveText('Musterberg Süd');
	await expect(metric(page, 'Time')).toHaveText('10:30');

	// Another member's plan: the pilot says "I am in"; changing it is its
	// author's alone, deleting it also the keeper's.
	await backToTheBoard(page).click();
	await plansOf(page, 'Today').filter({ hasText: 'Beispielwiese' }).getByRole('link').click();
	await expect(metric(page, 'Posted by')).toHaveText('Mira Member');
	await expect(page.getByRole('button', { name: 'Change the plan' })).toBeHidden();
	await expect(page.getByRole('button', { name: 'Delete the plan' })).toBeVisible();
	await page.getByRole('button', { name: 'I am in' }).click();
	await expect(whoIsIn).toHaveText(['You', 'Mira Member']);
	await backToTheBoard(page).click();
	const theirs = plansOf(page, 'Today').filter({ hasText: 'Beispielwiese' });
	await expect(theirs).toContainText('You and Mira Member');

	// The pilot's word is on the card as well: out, and in again.
	await imIn(theirs).click();
	await expect(imIn(theirs)).toHaveAttribute('aria-pressed', 'false');
	await expect(theirs).not.toContainText('You and Mira Member');
	await imIn(theirs).click();
	await expect(imIn(theirs)).toHaveAttribute('aria-pressed', 'true');
	await expect(theirs).toContainText('You and Mira Member');

	// Deleting a plan asks first, with what it means.
	await plansOf(page, 'Today').filter({ hasText: 'Musterberg Süd' }).getByRole('link').click();
	await page.getByRole('button', { name: 'Delete the plan' }).click();
	const deleting = page.getByRole('alertdialog', { name: 'Delete this plan?' });
	await expect(deleting).toContainText('It goes for everyone in the group, with who was in.');
	await deleting.getByRole('button', { name: 'Delete the plan' }).click();
	await expect(page.getByRole('heading', { level: 1, name: 'Community' })).toBeVisible();
	await expect(plansOf(page, 'Today')).toHaveText([/Beispielwiese/]);

	// The group: its members, and what its owner can do, in its settings.
	await page
		.getByRole('main')
		.getByRole('link', { name: /^Hausberg-Runde/ })
		.click();
	await expect(page.getByRole('heading', { level: 1, name: 'Hausberg-Runde' })).toBeVisible();
	const owning = await settingsOf(page);
	const members = owning.getByRole('list', { name: 'Members' }).getByRole('listitem');
	await expect(members).toHaveCount(2);
	const himself = members.filter({ hasText: 'Kira Pilot (you)' });
	const her = members.filter({ hasText: 'Mira Member' });
	await expect(himself.getByText('Owner', { exact: true })).toBeVisible();
	await expect(her.getByText('Owner', { exact: true })).toBeHidden();
	await keep('80-group');

	// Handing the group on is asked about first, with what it means.
	await owning.getByRole('button', { name: 'More about Mira Member' }).click();
	await page.getByRole('menuitem', { name: 'Make Mira Member the owner' }).click();
	const handing = page.getByRole('alertdialog', { name: 'Hand the group to Mira Member?' });
	await expect(handing).toContainText('You stay in the group as a member');
	await handing.getByRole('button', { name: 'Hand on' }).click();
	await expect(her.getByText('Owner', { exact: true })).toBeVisible();
	await expect(himself.getByText('Owner', { exact: true })).toBeHidden();
	// From here on he is a member like any other: nothing of the owner's is offered.
	await expect(owning.getByRole('textbox', { name: 'Invite by handle' })).toBeHidden();
	await expect(owning.getByRole('button', { name: 'Delete the group' })).toBeHidden();
	await expect(owning.getByRole('button', { name: /^More about/ })).toBeHidden();
	await owning.getByRole('button', { name: 'Done' }).click();

	// She owns the group now, and removes him: asked about first.
	await other.goto(groupPage);
	const hers = await settingsOf(other);
	await hers.getByRole('button', { name: 'More about Kira Pilot' }).click();
	await other.getByRole('menuitem', { name: 'Remove from the group' }).click();
	await other
		.getByRole('alertdialog', { name: 'Remove Kira Pilot?' })
		.getByRole('button', { name: 'Remove from the group' })
		.click();
	await expect(hers.getByRole('list', { name: 'Members' }).getByRole('listitem')).toHaveText([
		/Mira Member \(you\).*Owner/
	]);

	// To him the group is gone, and the board says that he is in none.
	await page.reload();
	await expect(page.getByRole('heading', { level: 1, name: 'Hausberg-Runde' })).toBeHidden();
	await openBoard(page);
	await expect(page.getByText('You are in no group yet. In a group')).toBeVisible();

	// She deletes the group: asked about first, and then it is gone for her too.
	await hers.getByRole('button', { name: 'Delete the group' }).click();
	const ending = other.getByRole('alertdialog', { name: 'Delete this group?' });
	await expect(ending).toContainText(
		'The group goes for every member, with its messages and plans.'
	);
	await ending.getByRole('button', { name: 'Delete the group' }).click();
	await expect(other.getByRole('heading', { level: 1, name: 'Community' })).toBeVisible();
	await expect(other.getByRole('link', { name: /^Hausberg-Runde/ })).toBeHidden();

	await others.close();
	expect(problems).toEqual([]);
});

test('a pilot is invited into a group, sees who is in it, and leaves it again', async ({
	page,
	keep
}) => {
	const handle = await signUp(page, 'Test Pilot');
	await intoMustergruppe(page, handle);

	// A member sees who is in the group and who owns it, and nothing of
	// what only the owner and the admins can do.
	const settings = await settingsOf(page);
	const members = settings.getByRole('list', { name: 'Members' }).getByRole('listitem');
	await expect(members.filter({ hasText: 'Demo Pilot' })).toContainText('Owner');
	await expect(members.filter({ hasText: 'Mara Muster' })).toContainText('@mara');
	await expect(members.filter({ hasText: 'Jonas Beispiel' })).toBeVisible();
	await expect(members.filter({ hasText: 'Test Pilot (you)' })).toBeVisible();
	await expect(settings.getByRole('textbox', { name: 'Invite by handle' })).toBeHidden();
	await expect(settings.getByRole('button', { name: 'Delete the group' })).toBeHidden();
	await expect(settings.getByRole('button', { name: /^More about/ })).toBeHidden();
	await keep('81-group-member');
	await settings.getByRole('button', { name: 'Done' }).click();

	// Leaving asks first, with what it means.
	await (await settingsOf(page)).getByRole('button', { name: 'Leave', exact: true }).click();
	const leaving = page.getByRole('alertdialog', { name: 'Leave this group?' });
	await expect(leaving).toContainText('You come back in only when invited again.');
	await leaving.getByRole('button', { name: 'Leave the group' }).click();
	await expect(page.getByRole('heading', { level: 1, name: 'Community' })).toBeVisible();
	await expect(
		page.getByRole('list', { name: 'Your groups' }).getByRole('listitem', { name: 'Mustergruppe' })
	).toHaveCount(0);
});

test('the board is asked for from the day on the pilot’s own clock, not UTC’s', async ({
	page
}) => {
	// Half past midnight in Munich on 21 November 2026, where the test
	// browser's clock is: UTC still has the 20th.
	await page.clock.setFixedTime(new Date('2026-11-20T23:30:00Z'));
	await intoMustergruppe(page, await signUp(page, 'Night Owl'));

	const boardAsked = page.waitForRequest((request) => request.url().includes('/v1/groups/plans?'));
	await openBoard(page);
	expect(new URL((await boardAsked).url()).search).toBe('?from=2026-11-21');

	// A plan made now is for the 21st, and stands under "Today". What a
	// pilot types is set as text, on the board and on the plan's page: a
	// place or a note is never read as markup.
	const place = `<img src=x onerror=alert(1)> Nachtwiese ${Math.random().toString(36).slice(2, 8)}`;
	await page.getByRole('button', { name: 'New plan' }).click();
	await expect(page.getByRole('dialog', { name: 'New plan' }).getByLabel('Day')).toHaveValue(
		'2026-11-21'
	);
	await savePlan(page, 'New plan', { where: place, note: '<b>Bring a torch.</b>' });
	const posted = plansOf(page, 'Today').filter({ hasText: place });
	await expect(posted).toBeVisible();
	// (Pilots' pictures, among the pilots near, are images of the server's.)
	await expect(page.getByRole('main').locator('img:not([src^="/v1/pictures/"])')).toHaveCount(0);
	await posted.getByRole('link').click();
	await expect(metric(page, 'Where')).toHaveText(place);
	await expect(page.getByText('<b>Bring a torch.</b>')).toBeVisible();
	await expect(page.getByRole('main').locator('img, b')).toHaveCount(0);
	await backToTheBoard(page).click();

	// Leaving takes the plan along: the group is as it was.
	await page.goto(`/community/groups/${MUSTERGRUPPE.id}/`);
	await leaveTheGroup(page);
});

test('a board of several days, and a plan that says where its place is', async ({ page, keep }) => {
	// The development mode's plans are for the day its clock shows and the
	// two after it, 18 to 20 November 2026. The browser's clock is set to
	// noon of that first day, so that it does not matter when this runs.
	await page.clock.setFixedTime(new Date('2026-11-18T11:00:00Z'));
	const handle = await signUp(page, 'Test Pilot');
	await intoMustergruppe(page, handle);
	await openBoard(page);

	// Day by day: today, tomorrow, then the weekday and the date.
	const first = plansOf(page, 'Today').filter({ hasText: 'Musterberg, Startplatz Ost' });
	await expect(first).toContainText('10:30');
	await expect(first).toContainText('Demo Pilot, Mara Muster and Jonas Beispiel');
	await expect(first).toContainText('Mustergruppe');
	// The made-up plans by their places written out in full: this test runs
	// at four widths, two at a time, and each posts a plan of its own for
	// tomorrow further down, at a place that begins the same.
	const madeUp = (day: string, place: string) =>
		plansOf(page, day).filter({ has: page.getByText(place, { exact: true }) });
	await expect(madeUp('Tomorrow', 'Beispielwiese')).toContainText('09:00');
	await expect(madeUp('Friday, 20 Nov', 'Musterberg')).toBeVisible();
	await keep('82-plans-board-days');

	// Another member's plan: to read, and to be in on.
	await first.getByRole('link').click();
	await expect(metric(page, 'Posted by')).toHaveText('Demo Pilot');
	await expect(page.getByText('At the lift at half past ten, in the air by noon.')).toBeVisible();
	await expect(page.getByRole('button', { name: 'I am in' })).toBeVisible();
	await expect(page.getByRole('button', { name: 'Change the plan' })).toBeHidden();
	await expect(page.getByRole('button', { name: 'Delete the plan' })).toBeHidden();

	// It says where its place is, so the forecast for it is one step away.
	await expect(page.getByText('47.6642° N, 11.7967° E')).toBeVisible();
	await page.getByRole('button', { name: 'Forecast for this place' }).click();
	const forecast = page.getByRole('dialog', { name: 'Forecast' });
	await expect(forecast).toContainText('Musterberg, Startplatz Ost');
	await expect(forecast.getByRole('region', { name: 'The forecast, hour by hour' })).toBeVisible();
	await keep('83-plan-forecast');
	await forecast.getByRole('button', { name: 'Done' }).click();

	// A plan of the pilot's own that a phone gave a position: the form
	// shows it, keeps it through a change, and lets the pilot take it off.
	const place = `Beispielwiese, Parkplatz ${Math.random().toString(36).slice(2, 8)}`;
	await postFromAPhone(page, handle, {
		day: '2026-11-19',
		place,
		position: { lat: 47.6922, lon: 11.7675 }
	});
	await backToTheBoard(page).click();
	await plansOf(page, 'Tomorrow').filter({ hasText: place }).getByRole('link').click();
	await expect(page.getByRole('button', { name: 'Forecast for this place' })).toBeVisible();
	await page.getByRole('button', { name: 'Change the plan' }).click();
	const changing = page.getByRole('dialog', { name: 'Change the plan' });
	await expect(changing).toContainText('47.6922° N, 11.7675° E');
	await changing.getByRole('switch', { name: 'Set a time' }).check();
	await changing.getByLabel('Time', { exact: true }).fill('08:15');
	await changing.getByRole('button', { name: 'Save' }).click();
	await expect(changing).toBeHidden();
	await expect(metric(page, 'Time')).toHaveText('08:15');
	await expect(page.getByRole('button', { name: 'Forecast for this place' })).toBeVisible();

	await page.getByRole('button', { name: 'Change the plan' }).click();
	await changing.getByRole('button', { name: 'Take the position off' }).click();
	await expect(changing).not.toContainText('47.6922° N');
	await changing.getByRole('button', { name: 'Save' }).click();
	await expect(changing).toBeHidden();
	await expect(page.getByRole('button', { name: 'Forecast for this place' })).toBeHidden();

	// Leaving takes the plan along: the group is as it was.
	await page.goto(`/community/groups/${MUSTERGRUPPE.id}/`);
	await leaveTheGroup(page);
});

test('a member reports a group someone else keeps, and another member’s plan', async ({
	page,
	keep,
	problems
}) => {
	// The development mode's plans are for 18 to 20 November 2026.
	await page.clock.setFixedTime(new Date('2026-11-18T11:00:00Z'));
	await intoMustergruppe(page, await signUp(page, 'Test Pilot'));

	// The group, from its page: the reasons of a group, not of a flight.
	await page.getByRole('button', { name: 'Report this group' }).click();
	const groupForm = page.getByRole('dialog', { name: 'Report this group' });
	await expect(groupForm.getByRole('radio')).toHaveCount(5);
	await expect(groupForm.getByRole('radio', { name: 'Not a real flight' })).toHaveCount(0);
	await expect(groupForm).toContainText('sees that it came from you');
	await groupForm.getByRole('radio').first().check();
	await keep('84-group-report');
	await groupForm.getByRole('button', { name: 'Send report' }).click();
	await expect(groupForm.getByRole('status')).toHaveText('Sent. Whoever runs Venti will read it.');
	await groupForm.getByRole('button', { name: 'Done' }).click();
	await expect(groupForm).toBeHidden();

	// Another member's plan, from its page.
	await openBoard(page);
	await plansOf(page, 'Today')
		.filter({ hasText: 'Musterberg, Startplatz Ost' })
		.getByRole('link')
		.click();
	await expect(metric(page, 'Posted by')).toHaveText('Demo Pilot');
	await page.getByRole('button', { name: 'Report this plan' }).click();
	const planForm = page.getByRole('dialog', { name: 'Report this plan' });
	await expect(planForm.getByRole('radio')).toHaveCount(5);
	await planForm.getByRole('radio').first().check();
	await keep('85-plan-report');
	await planForm.getByRole('button', { name: 'Send report' }).click();
	await expect(planForm.getByRole('status')).toHaveText('Sent. Whoever runs Venti will read it.');
	expect(problems).toEqual([]);
});
