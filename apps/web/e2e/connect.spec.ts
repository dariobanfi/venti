/**
 * The page on which a pilot lets a program read their flights
 * (`/connect/?request=venti_oar_…`), against the development server.
 *
 * That server gives out requests over plain HTTP because it listens on the
 * machine's own address, so the first test plays the program for real: it
 * registers one, asks for the server's page and takes the identifier from
 * the link there, as a pilot's click would. The program's own address is
 * made up (`assistant.example`) and the test browser answers for it.
 *
 * The other tests answer the page's question themselves, for what the
 * development server cannot be made to say on demand (a request about to
 * end, one answered already, a server that is busy), and say so as made up.
 *
 * A registration counts against the server's ten an hour for an address:
 * one test registers, once for each project.
 */

import { createHash } from 'node:crypto';
import type { APIRequestContext, Page } from '@playwright/test';
import { expect, test } from './helpers';
import { madeUpGoogle } from './made-up-google';

/** The development server itself: the authorization server's pages are not part of `/v1`. */
const SERVER = process.env.VENTI_DEV_SERVER ?? 'http://127.0.0.1:8080';

/** Where the made-up program is sent back to. */
const CALLBACK = 'https://assistant.example/oauth/callback';

/** An identifier as the server makes them; nobody was given this one. */
const NOBODYS = `venti_oar_${'0'.repeat(52)}`;

/** A handle nobody has used on this server. */
function newHandle(): string {
	return `pilot${Math.random().toString(36).slice(2, 10)}`;
}

/**
 * Plays a program up to the point where the pilot is asked: registers it,
 * asks for the server's page, and takes the request's identifier from the
 * link that leads to the web app.
 */
async function pendingRequest(request: APIRequestContext): Promise<string> {
	const described = await request.get(`${SERVER}/.well-known/oauth-protected-resource/mcp`);
	expect(described.ok(), 'the development server offers the connector').toBe(true);
	const { resource } = (await described.json()) as { resource: string };
	const registered = await request.post(`${SERVER}/oauth/register`, {
		data: { client_name: 'Assistant', redirect_uris: [CALLBACK] }
	});
	expect(registered.ok(), 'the program registers').toBe(true);
	const { client_id } = (await registered.json()) as { client_id: string };
	const challenge = createHash('sha256')
		.update('a-verifier-of-the-test-that-is-long-enough-0123456789')
		.digest('base64url');
	const asked = await request.get(`${SERVER}/oauth/authorize`, {
		params: {
			response_type: 'code',
			client_id,
			redirect_uri: CALLBACK,
			code_challenge: challenge,
			code_challenge_method: 'S256',
			resource,
			state: 'state-of-the-test'
		}
	});
	const link = /\/connect\/\?request=(venti_oar_[0-9a-z]+)/.exec(await asked.text());
	expect(link, 'the server’s page links to the web app').not.toBeNull();
	return link![1];
}

/** Keeps the screen light and dark. */
async function keepBoth(
	page: Page,
	keep: (name: string) => Promise<void>,
	number: number,
	name: string
) {
	await keep(`${number}-${name}`);
	await page.emulateMedia({ colorScheme: 'dark' });
	await keep(`${number + 1}-${name}-dark`);
	await page.emulateMedia({ colorScheme: 'light' });
}

/** What this browser keeps for the site, in both of its stores. */
const stored = (page: Page) =>
	page.evaluate(() => JSON.stringify([{ ...localStorage }, { ...sessionStorage }]));

test('a pilot signs in from the page, comes back to it, allows, and is sent to the program', async ({
	page,
	request,
	keep,
	problems
}) => {
	const id = await pendingRequest(request);
	// The program's own page, played by the test browser: it notes what arrived.
	const arrived: { url: string; referer: string | undefined }[] = [];
	await page.route('https://assistant.example/**', async (route) => {
		arrived.push({ url: route.request().url(), referer: route.request().headers().referer });
		await route.fulfill({ contentType: 'text/html', body: '<title>Assistant</title>Back.' });
	});

	// Nobody is signed in: what is asked, and the way to sign in.
	await page.goto(`/connect/?request=${id}`);
	await expect(
		page.getByRole('heading', { level: 1, name: 'Let this program read your flights?' })
	).toBeVisible();
	await expect(page.getByText(/calls itself “Assistant”.*the program's own claim/)).toBeVisible();
	await expect(page.getByText(/Sign in to Venti to answer/)).toBeVisible();
	await expect(page.getByRole('heading', { name: 'It may read' })).toBeVisible();
	await expect(page.getByText('assistant.example', { exact: true })).toBeVisible();
	await expect(page.getByRole('button', { name: 'Allow', exact: true })).toHaveCount(0);
	// The page tells nobody its address, and no part of the app links to it.
	await expect(page.locator('meta[name="referrer"]')).toHaveAttribute('content', 'no-referrer');
	await expect(page.locator('a[href*="connect"]')).toHaveCount(0);
	await keepBoth(page, keep, 279, 'connect-sign-in-first');

	// Through the app's own sign-in, with an account of the test's own, and back.
	await page.getByRole('button', { name: 'Sign in', exact: true }).click();
	await expect(page).toHaveURL(/\/settings\/account\/$/);
	expect(await stored(page)).not.toContain('venti_oar_');
	const handle = newHandle();
	await page.getByRole('radio', { name: 'New account' }).check();
	await page.getByRole('textbox', { name: 'Handle' }).fill(handle);
	await page.getByRole('textbox', { name: 'Name shown to others' }).fill('Connect Test');
	await page.getByRole('textbox', { name: 'Password' }).fill('a password of the browser test');
	await page.getByRole('button', { name: 'Create account' }).click();

	// The same address, with the same request, and now the two buttons.
	await expect(page).toHaveURL(new RegExp(`/connect/\\?request=${id}$`));
	await expect(page.getByTestId('answers-as')).toHaveText(`You answer as @${handle}.`);
	await expect(page.getByText(/withdraw this at any time/)).toBeVisible();
	const allow = page.getByRole('button', { name: 'Allow', exact: true });
	await expect(allow).toBeVisible();
	await expect(page.getByRole('button', { name: "Don't allow" })).toBeVisible();
	await expect(page.getByRole('button', { name: 'Sign in', exact: true })).toHaveCount(0);
	expect(await stored(page)).not.toContain('venti_oar_');
	// Nothing went wrong so far, and nothing was written to the console.
	expect(problems).toEqual([]);
	await keepBoth(page, keep, 281, 'connect-asking');

	// Allowed: the browser is at the program's address, with a code and the
	// program's own state, and was told nothing of where it came from.
	await allow.click();
	await page.waitForURL('https://assistant.example/**');
	expect(arrived).toHaveLength(1);
	const back = new URL(arrived[0].url);
	expect(back.origin + back.pathname).toBe(CALLBACK);
	expect(back.searchParams.get('code')).toMatch(/^venti_oac_/);
	expect(back.searchParams.get('state')).toBe('state-of-the-test');
	expect(arrived[0].referer).toBeUndefined();
});

test('a link that is not complete is a request that has ended, and nothing is asked', async ({
	page,
	keep,
	problems
}) => {
	const asked: string[] = [];
	page.on('request', (request) => {
		if (request.url().includes('/v1/oauth/')) asked.push(request.method());
	});
	for (const address of ['/connect/?request=venti_oar_tooshort', '/connect/']) {
		await page.goto(address);
		await expect(
			page.getByRole('heading', { level: 1, name: 'This request has ended' })
		).toBeVisible();
		await expect(page.getByText(/Go back to the program and start again/)).toBeVisible();
		await expect(page.locator('.connect').getByRole('button')).toHaveCount(0);
	}
	expect(asked).toEqual([]);
	expect(problems).toEqual([]);
	await keepBoth(page, keep, 283, 'connect-ended');
});

test('a request ends by itself, and one that no longer waits says what became of it', async ({
	page,
	keep
}) => {
	// Made up: a request from a program on this device, which ends twenty
	// seconds after the server's clock as the app reads it.
	const health = (await (await page.request.get('/v1/health')).json()) as { time: string };
	const ends = new Date(Date.parse(health.time) + 20_000).toISOString();
	let answer: { status: number; json: unknown } = {
		status: 200,
		json: {
			program: 'Made-up program',
			returns_to: '127.0.0.1',
			on_this_device: true,
			scopes: ['flights:read'],
			may_read: ['Your flights (made up for the test).'],
			may_not: 'Change anything (made up for the test).',
			lasts: 'Until you withdraw it (made up for the test).',
			expires_at: ends
		}
	};
	await page.route('**/v1/oauth/requests/**', (route) => route.fulfill(answer));

	await page.goto(`/connect/?request=${NOBODYS}`);
	await expect(page.getByRole('note')).toContainText('a program on the device this browser runs');
	await keepBoth(page, keep, 285, 'connect-on-this-device');
	await expect(page.getByRole('heading', { level: 1, name: 'This request has ended' })).toBeVisible(
		{
			timeout: 40_000
		}
	);
	await expect(page.locator('.connect').getByRole('button')).toHaveCount(0);

	// Made up: the server says the request was answered already.
	answer = {
		status: 410,
		json: { error: { code: 'authorization_request_answered', message: 'made up' } }
	};
	await page.goto(`/connect/?request=${NOBODYS}`);
	await expect(page.getByRole('heading', { level: 1, name: 'Already answered' })).toBeVisible();
	await expect(page.locator('.connect').getByRole('button')).toHaveCount(0);

	// Made up: a server that is busy, and then one that knows no such request.
	answer = { status: 503, json: {} };
	await page.goto(`/connect/?request=${NOBODYS}`);
	await expect(
		page.getByRole('heading', { level: 1, name: 'Venti could not be asked' })
	).toBeVisible();
	answer = {
		status: 404,
		json: { error: { code: 'authorization_request_not_found', message: 'made up' } }
	};
	await page.getByRole('button', { name: 'Try again' }).click();
	await expect(
		page.getByRole('heading', { level: 1, name: 'This request has ended' })
	).toBeVisible();
});

test('a pilot who signs in with Google comes back to the same request', async ({ page }) => {
	// Made up: a request that waits for ten minutes by the server's clock.
	// Google is the development server's made-up one (`made-up-google.ts`).
	const health = (await (await page.request.get('/v1/health')).json()) as { time: string };
	await page.route('**/v1/oauth/requests/**', (route) =>
		route.fulfill({
			json: {
				program: 'Made-up program',
				returns_to: 'assistant.example',
				on_this_device: false,
				scopes: ['flights:read'],
				may_read: ['Your flights (made up for the test).'],
				may_not: 'Change anything (made up for the test).',
				lasts: 'Until you withdraw it (made up for the test).',
				expires_at: new Date(Date.parse(health.time) + 600_000).toISOString()
			}
		})
	);
	const email = `connect.test.${Math.random().toString(36).slice(2, 8)}@example.com`;
	await madeUpGoogle(page, { email, name: 'Connect Test' });

	await page.goto(`/connect/?request=${NOBODYS}`);
	await page.getByRole('button', { name: 'Sign in', exact: true }).click();
	await expect(page).toHaveURL(/\/settings\/account\/$/);

	// To Google and back to the account's page, then the handle of a new account.
	await page.getByRole('button', { name: 'Continue with Google' }).click();
	await expect(page.getByRole('heading', { name: 'Choose your handle' })).toBeVisible();
	const handle = await page.getByRole('textbox', { name: 'Handle' }).inputValue();
	await page.getByRole('button', { name: 'Create account' }).click();

	// Two pages back in the tab's history: the same address, now with the two buttons.
	await expect(page).toHaveURL(new RegExp(`/connect/\\?request=${NOBODYS}$`));
	await expect(page.getByTestId('answers-as')).toHaveText(`You answer as @${handle}.`);
	await expect(page.getByRole('button', { name: 'Allow', exact: true })).toBeVisible();
	expect(await stored(page)).not.toContain('venti_oar_');
});

/**
 * What is drawn wider than a phone of 360 px inside `within`, and the page
 * itself if it can be moved sideways. The width is the phone's own: a page
 * that is too wide makes the test browser's window wider with it.
 */
const tooWide = (page: Page, within: string) =>
	page.evaluate((within) => {
		const wide = [...document.querySelectorAll(`${within} *`)]
			.filter((element) => {
				const box = element.getBoundingClientRect();
				return box.width > 0 && box.right > 361;
			})
			.map((element) => `${element.tagName.toLowerCase()}.${element.className}`);
		if (document.documentElement.scrollWidth > 361) wide.push('the page');
		return wide;
	}, within);

test('the page fits a narrow phone with large text', async ({ page, keep, wide }) => {
	test.skip(wide, 'A narrow phone: 360 px wide, text at 130 %.');
	await page.setViewportSize({ width: 360, height: 780 });
	// Made up: a request from a program on this device, with the warning.
	const health = (await (await page.request.get('/v1/health')).json()) as { time: string };
	await page.route('**/v1/oauth/requests/**', (route) =>
		route.fulfill({
			json: {
				program: 'Made-up program with a long name of its own',
				returns_to: 'a-long-host-name.of-a-made-up-program.example',
				on_this_device: true,
				scopes: ['flights:read'],
				may_read: ['Your flights (made up for the test).'],
				may_not: 'Change anything (made up for the test).',
				lasts: 'Until you withdraw it (made up for the test).',
				expires_at: new Date(Date.parse(health.time) + 600_000).toISOString()
			}
		})
	);
	await page.goto(`/connect/?request=${NOBODYS}`);
	await page.evaluate(() => (document.documentElement.style.fontSize = '130%'));
	await expect(page.getByRole('note')).toBeVisible();
	expect(await tooWide(page, '.connect')).toEqual([]);
	await keep('292-connect-narrow');
	const signIn = page.getByRole('button', { name: 'Sign in', exact: true });
	await signIn.scrollIntoViewIfNeeded();
	await expect(signIn).toBeInViewport();
	await keep('293-connect-narrow-button');
});
