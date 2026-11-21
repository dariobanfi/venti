/**
 * Signing in with Google by Google's OpenID Connect redirect, done by the
 * app itself: no script of Google's runs on the site. The button is the
 * app's own; it takes the pilot to Google's page, which comes back to the
 * account screen with an ID token in the address's fragment. The page
 * takes it out of the address at once, checks that it answers the request
 * this browser made (the `state` and the token's `nonce`), and hands it to
 * the core, which sends it to the Venti server (`Core.googleSignInRequest`);
 * the app keeps it no longer than that.
 *
 * A navigation needs no entry in the content security policy, so the
 * policy names nothing of Google's.
 */

import { accountHref } from '#lib/app/routes.ts';

/**
 * The web client ID Google gave Venti. Not a secret: it is in every page.
 * Another build names its own with `VITE_GOOGLE_CLIENT_ID`.
 */
export const GOOGLE_CLIENT_ID: string =
	import.meta.env.VITE_GOOGLE_CLIENT_ID ||
	'490070141586-8k1a3mrggi5fb87iirrcfhq7eint5ef3.apps.googleusercontent.com';

/** Google's page that signs a pilot in and comes back with an ID token. */
export const GOOGLE_AUTHORIZE = 'https://accounts.google.com/o/oauth2/v2/auth';

/**
 * The audience of the ID tokens the Venti server's development mode makes
 * for its made-up Google (`GET /v1/dev/google-id-token`), which carry no
 * nonce. Only the development server takes them: every other server
 * refuses a token that is not for its own client ID.
 */
export const DEVELOPMENT_AUDIENCE = 'venti-development.apps.googleusercontent.com';

/** What the pilot went to Google for. */
export type GoogleIntent = 'signIn' | 'link' | 'delete';

/** What came back from Google: an ID token for what the pilot went there for, or why not. */
export type GoogleReturn =
	{ intent: GoogleIntent; idToken: string } | { intent: GoogleIntent | undefined; refusal: string };

/** What the browser keeps of a request to Google until it comes back, in `sessionStorage`. */
const KEPT = 'venti.google-sign-in';

interface Kept {
	state: string;
	nonce: string;
	intent: GoogleIntent;
}

/** The address Google comes back to: the account screen, with its slash. */
export function redirectUri(origin: string = location.origin): string {
	return `${origin}${accountHref().replace(/\/$/, '')}/`;
}

/** Sixteen random bytes, written for an address. */
function random(): string {
	const bytes = crypto.getRandomValues(new Uint8Array(16));
	return base64url(bytes);
}

function base64url(bytes: Uint8Array): string {
	return btoa(String.fromCharCode(...bytes))
		.replaceAll('+', '-')
		.replaceAll('/', '_')
		.replace(/=+$/, '');
}

/** The address of Google's page for a request with this `state` and `nonce`. */
export function authorizeUrl(state: string, nonce: string, origin?: string): string {
	const query = new URLSearchParams({
		client_id: GOOGLE_CLIENT_ID,
		response_type: 'id_token',
		scope: 'openid email profile',
		redirect_uri: redirectUri(origin),
		nonce,
		state,
		prompt: 'select_account'
	});
	return `${GOOGLE_AUTHORIZE}?${query}`;
}

/** Takes the pilot to Google, for `intent`. The page comes back to the account screen. */
export function goToGoogle(intent: GoogleIntent, storage: Storage = sessionStorage): void {
	const kept: Kept = { state: random(), nonce: random(), intent };
	storage.setItem(KEPT, JSON.stringify(kept));
	location.assign(authorizeUrl(kept.state, kept.nonce));
}

/** The claims of an ID token, read without checking its signature: the server checks that. */
export function claimsOf(idToken: string): Record<string, unknown> | undefined {
	const payload = idToken.split('.')[1];
	if (!payload) return undefined;
	try {
		const text = atob(payload.replaceAll('-', '+').replaceAll('_', '/'));
		const claims: unknown = JSON.parse(
			new TextDecoder().decode(Uint8Array.from(text, (char) => char.charCodeAt(0)))
		);
		return claims && typeof claims === 'object' ? (claims as Record<string, unknown>) : undefined;
	} catch {
		return undefined;
	}
}

/** Said when what came back does not answer the request this browser made. */
const NOT_OURS = 'The sign-in with Google did not come back as it was sent. Try again.';

/**
 * Reads what Google sent back in the address's `fragment`, if it is an
 * answer of Google's: an ID token that answers the request this browser
 * kept, or why it is not one. Nothing when the fragment is no answer of
 * Google's, or when the pilot turned back at Google's page. What was kept
 * of the request is forgotten either way: a request is answered once.
 */
export function readGoogleReturn(
	fragment: string,
	storage: Storage = sessionStorage
): GoogleReturn | undefined {
	const said = new URLSearchParams(fragment.replace(/^#/, ''));
	if (!said.has('id_token') && !said.has('error') && !said.has('state')) return undefined;
	let kept: Kept | undefined;
	try {
		kept = JSON.parse(storage.getItem(KEPT) ?? 'null') ?? undefined;
	} catch {
		kept = undefined;
	}
	storage.removeItem(KEPT);
	const intent = kept?.intent;
	if (!kept || said.get('state') !== kept.state) return { intent, refusal: NOT_OURS };
	const idToken = said.get('id_token');
	// The pilot turned back, or Google said no: nothing happened.
	if (!idToken) return undefined;
	const claims = claimsOf(idToken);
	if (!claims) return { intent, refusal: NOT_OURS };
	// The nonce ties the token to this request. Only the development mode's
	// made-up Google leaves it out, whose tokens no other server takes.
	if (claims.nonce !== kept.nonce && claims.aud !== DEVELOPMENT_AUDIENCE) {
		return { intent, refusal: NOT_OURS };
	}
	return { intent: kept.intent, idToken };
}

/**
 * Takes Google's answer out of the address bar at once, so that the token
 * is neither in the history nor in a link the pilot copies, and reads it.
 */
export function takeGoogleReturn(): GoogleReturn | undefined {
	if (typeof location === 'undefined' || location.hash === '') return undefined;
	const fragment = location.hash;
	const answered = readGoogleReturn(fragment);
	if (answered || /(^#|&)(id_token|state|error)=/.test(fragment)) {
		history.replaceState(history.state, '', `${location.pathname}${location.search}`);
	}
	return answered;
}
