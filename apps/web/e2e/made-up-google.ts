/**
 * Google Identity Services, as the tests see it. The app is not told: it
 * asks Google for its script as it always does, and the test browser
 * answers instead.
 *
 * Every test gets a script that holds nothing (`noGoogle`), so that no test
 * reaches Google and every screen looks as it does where Google's script
 * does not load: the account screen opens its handle and password form at
 * once. A test of signing in with Google gets a made-up Google
 * (`madeUpGoogle`) whose button takes an ID token from the development
 * server's own made-up Google (`GET /v1/dev/google-id-token`) for the email
 * it was given, and hands it to the app as Google's would. The server
 * checks it as it checks Google's.
 */

import type { BrowserContext, Page } from '@playwright/test';

/** Where the app asks for Google Identity Services (`src/lib/account/google.ts`). */
const SCRIPT = 'https://accounts.google.com/gsi/client';

/** No Google: the script loads and gives the page nothing. */
export async function noGoogle(context: BrowserContext): Promise<void> {
	await context.route(SCRIPT, (route) =>
		route.fulfill({
			contentType: 'text/javascript',
			body: '/* Google Identity Services is not reached from the tests. */'
		})
	);
}

/**
 * A made-up Google on this page whose button signs in as the Google
 * account of `email`. It is drawn by the page's script from the app's
 * call, with Google's words; its look is a likeness of Google's, set
 * through the element's style so that the site's policy lets it.
 */
export async function madeUpGoogle(
	page: Page,
	account: { email: string; name: string }
): Promise<void> {
	const script = `(() => {
	const ACCOUNT = ${JSON.stringify(account)};
	let config;
	const look = (dark) => ({
		display: 'flex',
		alignItems: 'center',
		justifyContent: 'center',
		gap: '10px',
		width: '100%',
		height: '40px',
		borderRadius: '20px',
		border: dark ? '1px solid #8e918f' : '1px solid #747775',
		background: dark ? '#131314' : '#ffffff',
		color: dark ? '#e3e3e3' : '#1f1f1f',
		font: '500 14px/20px sans-serif'
	});
	window.google = { accounts: { id: {
		initialize(given) { config = given; },
		disableAutoSelect() {},
		renderButton(parent, options) {
			const button = document.createElement('button');
			button.type = 'button';
			button.textContent = 'Continue with Google';
			Object.assign(button.style, look(options.theme === 'filled_black'));
			if (options.width) button.style.maxWidth = options.width + 'px';
			button.addEventListener('click', async () => {
				const asked = new URLSearchParams({ email: ACCOUNT.email, name: ACCOUNT.name });
				const answer = await fetch('/v1/dev/google-id-token?' + asked);
				const { id_token } = await answer.json();
				config.callback({ credential: id_token, select_by: 'btn' });
			});
			parent.append(button);
		}
	} } };
})();`;
	await page.route(SCRIPT, (route) =>
		route.fulfill({ contentType: 'text/javascript', body: script })
	);
}
