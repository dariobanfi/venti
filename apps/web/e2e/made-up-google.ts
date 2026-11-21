/**
 * Google's sign-in page, as the tests see it. The app is not told: the
 * pilot's button takes the browser to Google's page as it always does
 * (`src/lib/account/google.ts`), and the test browser answers for Google,
 * sending it straight back to the address it named with an ID token in
 * the fragment, as Google does once a pilot has chosen an account.
 *
 * The token is one of the development server's own made-up Google
 * (`GET /v1/dev/google-id-token`), which the server checks as it checks
 * Google's. It carries no nonce: the app lets that pass only for the
 * development mode's audience, which no other server takes.
 */

import type { Page } from '@playwright/test';

/** Google's page that signs a pilot in. */
const AUTHORIZE = 'https://accounts.google.com/o/oauth2/v2/auth**';

/** What the app asked Google for, each time, as the query said it. */
export type AskedOfGoogle = Record<string, string>;

/**
 * A made-up Google on this page, which signs in as the Google account of
 * `email`. Returns what the app asked of it, each time it was asked.
 */
export async function madeUpGoogle(
	page: Page,
	account: { email: string; name: string }
): Promise<AskedOfGoogle[]> {
	const asked: AskedOfGoogle[] = [];
	await page.route(AUTHORIZE, async (route) => {
		const query = Object.fromEntries(new URL(route.request().url()).searchParams);
		asked.push(query);
		const answer = await page.request.get(
			`/v1/dev/google-id-token?${new URLSearchParams({ email: account.email, name: account.name })}`
		);
		const { id_token } = (await answer.json()) as { id_token: string };
		const back = new URLSearchParams({ id_token, state: query.state ?? '' });
		await route.fulfill({
			status: 302,
			headers: { location: `${query.redirect_uri}#${back}` }
		});
	});
	return asked;
}
