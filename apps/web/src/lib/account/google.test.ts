import { describe, expect, it } from 'vitest';
import { authorizeUrl, claimsOf, DEVELOPMENT_AUDIENCE, readGoogleReturn } from './google';

/** A storage that lives in memory, as `sessionStorage` does for a tab. */
function memory(): Storage {
	const kept = new Map<string, string>();
	return {
		get length() {
			return kept.size;
		},
		clear: () => kept.clear(),
		getItem: (key) => kept.get(key) ?? null,
		key: (index) => [...kept.keys()][index] ?? null,
		removeItem: (key) => void kept.delete(key),
		setItem: (key, value) => void kept.set(key, value)
	};
}

/** An ID token with these claims; its signature is the server's to check. */
function token(claims: Record<string, unknown>): string {
	const part = (value: unknown) =>
		btoa(String.fromCharCode(...new TextEncoder().encode(JSON.stringify(value))))
			.replaceAll('+', '-')
			.replaceAll('/', '_')
			.replace(/=+$/, '');
	return `${part({ alg: 'RS256' })}.${part(claims)}.signature`;
}

const KEPT = { state: 'the-state', nonce: 'the-nonce', intent: 'link' };

function keeping(kept: unknown = KEPT): Storage {
	const storage = memory();
	storage.setItem('venti.google-sign-in', JSON.stringify(kept));
	return storage;
}

const fragment = (said: Record<string, string>) => `#${new URLSearchParams(said)}`;

describe('signing in with Google by its redirect', () => {
	it('asks Google for an ID token for the web client, back to the account screen', () => {
		const url = new URL(authorizeUrl('s', 'n', 'https://venti.fly'));
		expect(url.origin + url.pathname).toBe('https://accounts.google.com/o/oauth2/v2/auth');
		expect(Object.fromEntries(url.searchParams)).toEqual({
			client_id: '490070141586-8k1a3mrggi5fb87iirrcfhq7eint5ef3.apps.googleusercontent.com',
			response_type: 'id_token',
			scope: 'openid email profile',
			redirect_uri: 'https://venti.fly/settings/account/',
			nonce: 'n',
			state: 's',
			prompt: 'select_account'
		});
	});

	it('takes a token that answers the request kept, for what it was asked for, once', () => {
		const storage = keeping();
		const idToken = token({ nonce: 'the-nonce', aud: 'web' });
		expect(readGoogleReturn(fragment({ id_token: idToken, state: 'the-state' }), storage)).toEqual({
			intent: 'link',
			idToken
		});
		expect(storage.length).toBe(0);
		// Read again, it answers nothing any more.
		expect(
			readGoogleReturn(fragment({ id_token: idToken, state: 'the-state' }), storage)
		).toMatchObject({ refusal: expect.any(String) });
	});

	it('refuses a token for another request: another state, or another nonce', () => {
		const idToken = token({ nonce: 'the-nonce' });
		expect(
			readGoogleReturn(fragment({ id_token: idToken, state: 'other' }), keeping())
		).toMatchObject({ intent: 'link', refusal: expect.stringContaining('Try again') });
		const stranger = token({ nonce: 'other' });
		expect(
			readGoogleReturn(fragment({ id_token: stranger, state: 'the-state' }), keeping())
		).toMatchObject({ refusal: expect.any(String) });
		const none = token({ aud: 'web' });
		expect(
			readGoogleReturn(fragment({ id_token: none, state: 'the-state' }), keeping())
		).toMatchObject({ refusal: expect.any(String) });
		expect(
			readGoogleReturn(fragment({ id_token: 'not a token', state: 'the-state' }), keeping())
		).toMatchObject({ refusal: expect.any(String) });
	});

	it('lets a token of the development mode’s made-up Google through without a nonce', () => {
		const idToken = token({ aud: DEVELOPMENT_AUDIENCE });
		expect(
			readGoogleReturn(fragment({ id_token: idToken, state: 'the-state' }), keeping())
		).toEqual({ intent: 'link', idToken });
	});

	it('says nothing when the pilot turned back at Google, or the fragment is not Google’s', () => {
		const storage = keeping();
		expect(
			readGoogleReturn(fragment({ error: 'access_denied', state: 'the-state' }), storage)
		).toBeUndefined();
		expect(storage.length).toBe(0);
		expect(readGoogleReturn('#fired', keeping())).toBeUndefined();
	});

	it('reads the claims of a token, names in other scripts too', () => {
		expect(claimsOf(token({ name: 'Zoë 李' }))).toMatchObject({ name: 'Zoë 李' });
	});
});
