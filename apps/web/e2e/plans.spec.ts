/**
 * Flying plans: the board of who means to fly where, a plan with who is
 * in, the form for a plan, and the groups, with what a keeper and a member
 * can do. The server is the development mode's, which keeps groups and
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
import { expect, metric, test } from './helpers';

/** The development mode's group that the demo pilot keeps, and the code that lets a pilot in. */
const MUSTERGRUPPE = {
	id: '44444444444444444444444444',
	name: 'Mustergruppe',
	code: '4444-4444-4444-4444'
};

const PASSWORD = 'correct horse battery staple';

const NEEDS_ACCOUNT =
	'Plans for a flying day are shared in groups on Venti, and a group is seen by its members only. Sign in under Settings, Account, then make a group or join one with the code its keeper gives you.';

const field = (page: Page, name: string) => page.getByRole('textbox', { name });

/** The navigation, where "Plans" is a section beside the map and the logbook. */
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

/** Opens the board from wherever the pilot is, without loading the site anew. */
async function openBoard(page: Page): Promise<void> {
	await sections(page).getByRole('link', { name: 'Plans', exact: true }).click();
	await expect(page.getByRole('heading', { level: 1, name: 'Flying plans' })).toBeVisible();
}

/** A day by the browser's own clock, as a date field writes it: today, or some days on. */
function dayByTheBrowser(page: Page, daysOn = 0): Promise<string> {
	return page.evaluate((daysOn) => {
		const day = new Date();
		day.setDate(day.getDate() + daysOn);
		const two = (value: number) => String(value).padStart(2, '0');
		return `${day.getFullYear()}-${two(day.getMonth() + 1)}-${two(day.getDate())}`;
	}, daysOn);
}

/** The plans listed under a day of the board, by what the day is called. */
const plansOf = (page: Page, day: string): Locator =>
	page
		.getByRole('list', { name: day, exact: true })
		.getByRole('listitem')
		.or(page.getByRole('table', { name: day, exact: true }).locator('tbody > tr'));

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

/** Joins a group with its code, from the list of groups. */
async function join(page: Page, code: string): Promise<Locator> {
	await page.getByRole('button', { name: 'Join with a code' }).click();
	const asking = page.getByRole('alertdialog', { name: 'Join a group' });
	await asking.getByRole('textbox', { name: 'Invite code' }).fill(code);
	await asking.getByRole('button', { name: 'Join' }).click();
	return asking;
}

test('signed out, the board says that plans need an account', async ({ page, keep, problems }) => {
	await page.goto('/');
	await openBoard(page);
	await expect(page).toHaveTitle('Flying plans · Venti');

	await expect(page.getByText(NEEDS_ACCOUNT)).toBeVisible();
	// Nothing to plan with, and nothing asked of the server.
	await expect(page.getByRole('button', { name: 'New plan' })).toBeHidden();
	await expect(page.getByRole('link', { name: 'Groups' })).toBeHidden();
	await keep('74-plans-signed-out');

	// The groups and a plan have addresses of their own, and say the same.
	await page.goto('/plans/groups/');
	await expect(page.getByText(NEEDS_ACCOUNT)).toBeVisible();
	await expect(page.getByRole('button', { name: 'Make a group' })).toBeHidden();
	await page.goto('/plans/plan/?id=66666666666666666666666666');
	await expect(page.getByText(NEEDS_ACCOUNT)).toBeVisible();

	// The way to an account is one click.
	await page.getByRole('main').getByRole('link', { name: 'Account' }).click();
	await expect(page.getByRole('heading', { level: 1, name: 'Account' })).toBeVisible();
	expect(problems).toEqual([]);
});

test('a pilot makes a group, hands on its code, and plans a flying day with who comes in', async ({
	browser,
	context,
	page,
	keep,
	problems
}) => {
	await context.grantPermissions(['clipboard-read', 'clipboard-write']);
	await signUp(page, 'Kira Pilot');

	// Without a group there is nobody to plan with, and the board says so.
	await openBoard(page);
	await expect(page.getByText('You are in no group yet.')).toBeVisible();
	await expect(page.getByRole('button', { name: 'New plan' })).toBeHidden();
	await keep('75-plans-no-group');
	await page.getByRole('link', { name: 'Make or join a group' }).click();
	await expect(page.getByRole('heading', { level: 1, name: 'Groups' })).toBeVisible();
	await expect(
		page.getByText('A group is seen by its members and nobody else; it cannot be searched for.')
	).toBeVisible();

	// Making one asks for its name and says what keeping it means. A name
	// the server would refuse, the core refuses before it is asked.
	await page.getByRole('button', { name: 'Make a group' }).click();
	const naming = page.getByRole('alertdialog', { name: 'Make a group' });
	await expect(naming).toContainText(
		'You will keep it: you hand out its invite code and can remove a member.'
	);
	await expect(naming.getByRole('button', { name: 'Make' })).toBeDisabled();
	await naming.getByRole('textbox', { name: 'Its name' }).fill('x'.repeat(61));
	await naming.getByRole('button', { name: 'Make' }).click();
	await expect(naming.getByRole('alert')).toHaveText("A group's name has room for 60 characters.");
	await naming.getByRole('textbox', { name: 'Its name' }).fill('Hausberg-Runde');
	await naming.getByRole('button', { name: 'Make' }).click();

	// Its invite code is shown once, with what it is worth, and can be copied.
	const invite = page.getByRole('dialog', { name: 'Invite code' });
	await expect(invite.getByRole('heading', { name: 'Hausberg-Runde' })).toBeVisible();
	const shown = invite.getByText(/^[0-9a-z]{4}(-[0-9a-z]{4}){3}$/);
	const code = (await shown.textContent())!.trim();
	await expect(invite).toContainText(
		'It is shown this once: Venti keeps no copy that can be read.'
	);
	await invite.getByRole('button', { name: 'Copy the code' }).click();
	await expect(invite.getByRole('status')).toHaveText('Copied.');
	expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(code);
	await keep('76-plans-invite-code');
	await invite.getByRole('button', { name: 'Done' }).click();
	const group = page.getByRole('link', { name: /^Hausberg-Runde/ });
	await expect(group).toContainText('1 member · kept by you');

	// Another pilot, in a browser of their own, comes in with the code. A
	// code that is none is refused in the core's words.
	const others = await browser.newContext();
	const other = await others.newPage();
	await signUp(other, 'Mira Member');
	await other.goto('/plans/groups/');
	const asking = await join(other, '0000-0000-0000-0000');
	await expect(asking.getByRole('alert')).toContainText("This is not a group's invite code");
	await asking.getByRole('textbox', { name: 'Invite code' }).fill(code.toUpperCase());
	await asking.getByRole('button', { name: 'Join' }).click();
	await expect(other.getByRole('status')).toHaveText('You are in Hausberg-Runde now.');
	await expect(other.getByRole('link', { name: /^Hausberg-Runde/ })).toContainText(
		'2 members · kept by Kira Pilot'
	);

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
	await expect(mine.getByRole('img', { name: 'You are in' })).toBeVisible();
	await expect(
		plansOf(page, 'Today')
			.filter({ hasText: 'Beispielwiese' })
			.getByRole('img', { name: 'You are in' })
	).toBeHidden();
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
	await page.getByRole('link', { name: 'Flying plans' }).click();
	await plansOf(page, 'Today').filter({ hasText: 'Beispielwiese' }).getByRole('link').click();
	await expect(metric(page, 'Posted by')).toHaveText('Mira Member');
	await expect(page.getByRole('button', { name: 'Change the plan' })).toBeHidden();
	await expect(page.getByRole('button', { name: 'Delete the plan' })).toBeVisible();
	await page.getByRole('button', { name: 'I am in' }).click();
	await expect(whoIsIn).toHaveText(['You', 'Mira Member']);
	await page.getByRole('link', { name: 'Flying plans' }).click();
	await expect(plansOf(page, 'Today').filter({ hasText: 'Beispielwiese' })).toContainText(
		'You and Mira Member'
	);

	// Deleting a plan asks first, with what it means.
	await plansOf(page, 'Today').filter({ hasText: 'Musterberg Süd' }).getByRole('link').click();
	await page.getByRole('button', { name: 'Delete the plan' }).click();
	const deleting = page.getByRole('alertdialog', { name: 'Delete this plan?' });
	await expect(deleting).toContainText('It goes for everyone in the group, with who was in.');
	await deleting.getByRole('button', { name: 'Delete the plan' }).click();
	await expect(page.getByRole('heading', { level: 1, name: 'Flying plans' })).toBeVisible();
	await expect(plansOf(page, 'Today')).toHaveText([/Beispielwiese/]);

	// The group: its members, and what its keeper can do.
	await page.getByRole('link', { name: 'Groups' }).click();
	await page.getByRole('link', { name: /^Hausberg-Runde/ }).click();
	await expect(page.getByRole('heading', { level: 1, name: 'Hausberg-Runde' })).toBeVisible();
	const members = page.getByRole('list', { name: 'Members' }).getByRole('listitem');
	await expect(members).toHaveCount(2);
	// In the order they came in. The development mode's clock stands
	// still, so these two came in at the same moment, and which is listed
	// first is not for a test to say.
	const himself = members.filter({ hasText: 'Kira Pilot (you)' });
	const her = members.filter({ hasText: 'Mira Member' });
	await expect(himself.getByText('Keeper', { exact: true })).toBeVisible();
	await expect(her.getByText('Keeper', { exact: true })).toBeHidden();
	await expect(
		page.getByText('Removing a member ends the invite code, which they know.')
	).toBeVisible();
	// One's own is not reported.
	await expect(page.getByRole('button', { name: 'Report this group' })).toBeHidden();
	await keep('80-group');

	// Renaming says who will see the name.
	await page.getByRole('button', { name: 'Rename the group' }).click();
	const renaming = page.getByRole('alertdialog', { name: 'Rename the group' });
	await expect(renaming.getByRole('textbox', { name: 'Its name' })).toHaveValue('Hausberg-Runde');
	await renaming.getByRole('textbox', { name: 'Its name' }).fill('Hausberg-Runde Süd');
	await renaming.getByRole('button', { name: 'Rename' }).click();
	await expect(page.getByRole('heading', { level: 1, name: 'Hausberg-Runde Süd' })).toBeVisible();

	// A new invite code is asked about first: it ends the one handed out.
	await page.getByRole('button', { name: 'New invite code' }).click();
	const newCode = page.getByRole('alertdialog', { name: 'Make a new invite code?' });
	await expect(newCode).toContainText(
		'The code handed out before stops working. Members stay in the group.'
	);
	// The safe answer has the focus: Enter leaves the code as it is.
	await page.keyboard.press('Enter');
	await expect(newCode).toBeHidden();
	await expect(invite).toBeHidden();
	await page.getByRole('button', { name: 'New invite code' }).click();
	await newCode.getByRole('button', { name: 'Make a new code' }).click();
	const second = (await invite.getByText(/^[0-9a-z]{4}(-[0-9a-z]{4}){3}$/).textContent())!.trim();
	expect(second).not.toBe(code);
	await invite.getByRole('button', { name: 'Done' }).click();

	// Handing the keeping on is asked about first too, with what it means.
	await page.getByRole('button', { name: 'More about Mira Member' }).click();
	await page.getByRole('menuitem', { name: 'Make keeper' }).click();
	const handing = page.getByRole('alertdialog', { name: 'Hand the keeping to Mira Member?' });
	await expect(handing).toContainText('You stay in the group as a member');
	await handing.getByRole('button', { name: 'Make keeper' }).click();
	await expect(her.getByText('Keeper', { exact: true })).toBeVisible();
	await expect(himself.getByText('Keeper', { exact: true })).toBeHidden();
	// From here on he is a member like any other: nothing of the keeper's is offered.
	await expect(page.getByRole('button', { name: 'New invite code' })).toBeHidden();
	await expect(page.getByRole('button', { name: 'Delete the group' })).toBeHidden();
	await expect(page.getByRole('button', { name: /^More about/ })).toBeHidden();
	await expect(page.getByRole('button', { name: 'Leave the group' })).toBeVisible();

	// She keeps the group now, and removes him: asked about first, with
	// what goes with him and what becomes of the invite code.
	await other.goto('/plans/groups/');
	await other.getByRole('link', { name: /^Hausberg-Runde Süd/ }).click();
	await expect(other.getByRole('list', { name: 'Members' }).getByRole('listitem')).toHaveCount(2);
	await other.getByRole('button', { name: 'More about Kira Pilot' }).click();
	await other.getByRole('menuitem', { name: 'Remove from the group' }).click();
	const removing = other.getByRole('alertdialog', { name: 'Remove Kira Pilot?' });
	await expect(removing).toContainText('The invite code stops working too, because they know it');
	await removing.getByRole('button', { name: 'Remove from the group' }).click();
	await expect(other.getByRole('list', { name: 'Members' }).getByRole('listitem')).toHaveText([
		/Mira Member \(you\).*Keeper/
	]);

	// To him the group is gone, and the board says that he is in none.
	await page.reload();
	await expect(
		page.getByText('This group is no longer there, or you are no longer in it.')
	).toBeVisible();
	await openBoard(page);
	await expect(page.getByText('You are in no group yet.')).toBeVisible();

	// The code he made lets nobody in any more: removing a member ended it.
	await page.getByRole('link', { name: 'Make or join a group' }).click();
	const again = await join(page, second);
	await expect(again.getByRole('alert')).toContainText("This is not a group's invite code");
	await again.getByRole('button', { name: 'Cancel' }).click();

	// She deletes the group: asked about first, and then it is gone for her too.
	await other.getByRole('button', { name: 'Delete the group' }).click();
	const ending = other.getByRole('alertdialog', { name: 'Delete this group?' });
	await expect(ending).toContainText('The group goes for every member, with all its plans.');
	await ending.getByRole('button', { name: 'Delete the group' }).click();
	await expect(other.getByRole('heading', { level: 1, name: 'Groups' })).toBeVisible();
	await expect(other.getByRole('link', { name: /^Hausberg-Runde/ })).toBeHidden();

	await others.close();
	expect(problems).toEqual([]);
});

test('a pilot comes into a group with its code, sees who is in it, and leaves it again', async ({
	page,
	keep
}) => {
	await signUp(page, 'Test Pilot');
	await page.goto('/plans/groups/');

	// The development mode's group, which its demo pilot keeps.
	await join(page, MUSTERGRUPPE.code);
	await expect(page.getByRole('status')).toHaveText('You are in Mustergruppe now.');
	const group = page.getByRole('link', { name: /^Mustergruppe/ });
	await expect(group).toContainText('kept by Demo Pilot');
	// The same code again changes nothing, and the answer says so.
	await join(page, MUSTERGRUPPE.code);
	await expect(page.getByRole('status')).toHaveText('You were in Mustergruppe already.');

	// A member sees who is in the group and who keeps it, and nothing of
	// what only the keeper can do.
	await group.click();
	await expect(page.getByRole('heading', { level: 1, name: 'Mustergruppe' })).toBeVisible();
	const members = page.getByRole('list', { name: 'Members' }).getByRole('listitem');
	await expect(members.filter({ hasText: 'Demo Pilot' })).toContainText('Keeper');
	await expect(members.filter({ hasText: 'Mara Muster' })).toContainText('@mara');
	await expect(members.filter({ hasText: 'Jonas Beispiel' })).toBeVisible();
	await expect(members.filter({ hasText: 'Test Pilot (you)' })).toBeVisible();
	await expect(page.getByRole('button', { name: 'New invite code' })).toBeHidden();
	await expect(page.getByRole('button', { name: 'Rename the group' })).toBeHidden();
	await expect(page.getByRole('button', { name: 'Delete the group' })).toBeHidden();
	await expect(page.getByRole('button', { name: /^More about/ })).toBeHidden();
	await expect(
		page.getByText('Your plans, and your word on the plans of others, go with you.')
	).toBeVisible();
	await keep('81-group-member');

	// Leaving asks first, with what it means.
	await page.getByRole('button', { name: 'Leave the group' }).click();
	const leaving = page.getByRole('alertdialog', { name: 'Leave this group?' });
	await expect(leaving).toHaveText(/You will no longer see its plans, and yours are removed\./);
	await expect(leaving).not.toContainText('keeps the group');
	await leaving.getByRole('button', { name: 'Leave the group' }).click();
	await expect(page.getByRole('heading', { level: 1, name: 'Groups' })).toBeVisible();
	await expect(group).toBeHidden();
});

test('the board is asked for from the day on the pilot’s own clock, not UTC’s', async ({
	page
}) => {
	// Half past midnight in Munich on 21 November 2026, where the test
	// browser's clock is: UTC still has the 20th.
	await page.clock.setFixedTime(new Date('2026-11-20T23:30:00Z'));
	await signUp(page, 'Night Owl');
	await page.goto('/plans/groups/');
	await join(page, MUSTERGRUPPE.code);
	await expect(page.getByRole('status')).toHaveText('You are in Mustergruppe now.');

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
	await expect(page.getByRole('main').locator('img')).toHaveCount(0);
	await posted.getByRole('link').click();
	await expect(metric(page, 'Where')).toHaveText(place);
	await expect(page.getByText('<b>Bring a torch.</b>')).toBeVisible();
	await expect(page.getByRole('main').locator('img, b')).toHaveCount(0);
	await page.getByRole('link', { name: 'Flying plans' }).click();

	// Leaving takes the plan along: the group is as it was.
	await page.getByRole('link', { name: 'Groups' }).click();
	await page.getByRole('link', { name: /^Mustergruppe/ }).click();
	await page.getByRole('button', { name: 'Leave the group' }).click();
	await page
		.getByRole('alertdialog', { name: 'Leave this group?' })
		.getByRole('button', { name: 'Leave the group' })
		.click();
	await expect(page.getByRole('heading', { level: 1, name: 'Groups' })).toBeVisible();
});

test('a board of several days, and a plan that says where its place is', async ({ page, keep }) => {
	// The development mode's plans are for the day its clock shows and the
	// two after it, 18 to 20 November 2026. The browser's clock is set to
	// noon of that first day, so that it does not matter when this runs.
	await page.clock.setFixedTime(new Date('2026-11-18T11:00:00Z'));
	const handle = await signUp(page, 'Test Pilot');
	await page.goto('/plans/groups/');
	await join(page, MUSTERGRUPPE.code);
	await expect(page.getByRole('status')).toHaveText('You are in Mustergruppe now.');
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
	await page.getByRole('link', { name: 'Flying plans' }).click();
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
	await page.getByRole('link', { name: 'Flying plans' }).click();
	await page.getByRole('link', { name: 'Groups' }).click();
	await page.getByRole('link', { name: /^Mustergruppe/ }).click();
	await page.getByRole('button', { name: 'Leave the group' }).click();
	await page
		.getByRole('alertdialog', { name: 'Leave this group?' })
		.getByRole('button', { name: 'Leave the group' })
		.click();
	await expect(page.getByRole('heading', { level: 1, name: 'Groups' })).toBeVisible();
});

test('a member reports a group someone else keeps, and another member’s plan', async ({
	page,
	keep,
	problems
}) => {
	// The development mode's plans are for 18 to 20 November 2026.
	await page.clock.setFixedTime(new Date('2026-11-18T11:00:00Z'));
	await signUp(page, 'Test Pilot');
	await page.goto('/plans/groups/');
	await join(page, MUSTERGRUPPE.code);
	await expect(page.getByRole('status')).toHaveText('You are in Mustergruppe now.');

	// The group, from beside "Leave the group": the reasons of a group, not of a flight.
	await page.getByRole('link', { name: /^Mustergruppe/ }).click();
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
