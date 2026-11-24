import { readFileSync } from 'node:fs';
import { IDBFactory } from 'fake-indexeddb';
import { afterEach, beforeAll, describe, expect, it } from 'vitest';
import {
	AccountSource,
	deviceName,
	type Send,
	type ServerAnswer,
	wholeSeconds
} from './account-source';
import { IdbStore } from './idb-store';
import { type ServerRequest, sitesMap } from './pkg/venti_core_wasm.js';
import { UNREACHABLE, unreachable as unreachableError } from './protocol';
import { CoreSession } from './session';
import { sampleFlight, startCore } from './testing';

beforeAll(startCore);

/** The server's own recorded answers, which its tests keep current. */
function recorded(name: string): Uint8Array {
	return readFileSync(
		new URL(`../../../../../crates/server/tests/golden/${name}`, import.meta.url)
	);
}

const json = (value: unknown) => new TextEncoder().encode(JSON.stringify(value));
const text = (bytes: Uint8Array | undefined) => new TextDecoder().decode(bytes);

/** The token the recorded sign-in hands out. */
const TOKEN = 'venti_3x7k9q2m4r8t1v6w0y5a2b3c4d5e6f7g8h9j0kmnpqrstvwxyz01';

const SERVER = 'https://venti.example';

/** 18 November 2026, 19:05 UTC, in milliseconds. */
const NOW = 1_795_028_700_000;

/** A list of changes with nothing after it. */
function changes(given: { flights?: unknown[]; deleted?: unknown[] } = {}): ServerAnswer {
	return answer(
		200,
		json({ flights: [], deleted: [], cursor: 'end', more: false, reset: false, ...given })
	);
}

/** The account's list of kept sites, with nothing after it. */
function keptChanges(given: { kept?: unknown[]; forgotten?: string[] } = {}): ServerAnswer {
	return answer(
		200,
		json({ kept: [], forgotten: [], cursor: '1', more: false, reset: false, ...given })
	);
}

function answer(status: number, body: Uint8Array = new Uint8Array()): ServerAnswer {
	return { status, body, retryAfterS: undefined };
}

function refusal(status: number, code: string): ServerAnswer {
	return answer(status, json({ error: { code, message: 'for developers' } }));
}

/**
 * A Venti server played from the recorded answers. It notes what it was
 * asked and with which token. `instead` answers a request differently, or
 * throws to be out of reach.
 */
class Server {
	asked: { method: string; path: string; token: string | undefined; body: string }[] = [];
	/** What the server's list of changes says. */
	changes = changes();
	/** What the account's list of kept sites says has changed. */
	keptChanges = keptChanges();
	instead: (request: ServerRequest) => ServerAnswer | undefined = () => undefined;

	send: Send = async (request, token) => {
		this.asked.push({
			method: request.method,
			path: request.path,
			// Only a request that needs the token may be given it.
			token: request.needsToken ? token : undefined,
			body: text(request.body)
		});
		const { method, path } = request;
		const other = this.instead(request);
		if (other) return other;
		if (method === 'POST' && (path === '/v1/sessions' || path === '/v1/accounts')) {
			return answer(201, recorded('sign_in.json'));
		}
		if (method === 'GET' && path.startsWith('/v1/flights/changes')) return this.changes;
		if (method === 'GET' && path.startsWith('/v1/account/kept-sites/changes')) {
			return this.keptChanges;
		}
		if (method === 'PUT' && path.startsWith('/v1/account/kept-sites/')) {
			// The site as sent, kept from the moment the request names or from now.
			const kept = JSON.parse(text(request.body));
			return answer(200, json({ kept: { kept_at: '2026-11-18T19:05:00Z', ...kept } }));
		}
		if (method === 'GET' && path.endsWith('/igc')) {
			return answer(200, new TextEncoder().encode(sampleFlight()));
		}
		if (method === 'POST' && path === '/v1/flights') return answer(201, recorded('upload.json'));
		if (method === 'PATCH') return answer(200, recorded('flight.json'));
		if (method === 'DELETE') return answer(204);
		return refusal(404, 'not_found');
	};

	/** What was asked, in short. */
	get lines(): string[] {
		return this.asked.map(({ method, path }) => `${method} ${path.split('?')[0]}`);
	}
}

/** One browser's storage for the site. Every tab opened on it shares it. */
class Browser {
	#factory = new IDBFactory();
	#open: CoreSession[] = [];
	server = new Server();

	async tab(): Promise<{ session: CoreSession; account: AccountSource }> {
		const session = await CoreSession.open(await IdbStore.open('venti', this.#factory));
		this.#open.push(session);
		const account = new AccountSource(session, this.server.send, SERVER, 'Test browser', () => NOW);
		return { session, account };
	}

	/** What the database holds, read behind the app's back. */
	async stored(): Promise<{ names: string[]; token: string | undefined }> {
		const store = await IdbStore.open('venti', this.#factory);
		const { names } = await store.snapshot();
		const token = await store.token();
		store.close();
		return { names: names.sort(), token };
	}

	closeTabs(): void {
		for (const session of this.#open.splice(0)) session.close();
	}
}

let browser: Browser;
afterEach(() => browser?.closeTabs());

describe('signing in', () => {
	it('stores the account and its token together, the token apart from the core’s files', async () => {
		browser = new Browser();
		const { account } = await browser.tab();
		expect(await account.account()).toBeUndefined();

		const signedIn = await account.signIn('demo', 'demo-pilot-password');

		expect(signedIn).toMatchObject({
			handle: 'demo',
			displayName: 'Demo Pilot',
			server: SERVER,
			needsSignIn: false
		});
		expect(browser.server.asked).toEqual([
			{
				method: 'POST',
				path: '/v1/sessions',
				token: undefined,
				body: '{"handle":"demo","password":"demo-pilot-password","device":"Test browser"}'
			}
		]);
		const stored = await browser.stored();
		expect(stored.names).toEqual(['account.json', 'logbook-index.json']);
		expect(stored.token).toBe(TOKEN);
	});

	it('is still signed in after a reload', async () => {
		browser = new Browser();
		await (await browser.tab()).account.signIn('demo', 'demo-pilot-password');
		browser.closeTabs();

		const { account } = await browser.tab();

		expect((await account.account())?.handle).toBe('demo');
	});

	it('says in the core’s words why the server refused, and stores nothing', async () => {
		browser = new Browser();
		const { account } = await browser.tab();
		browser.server.instead = () => refusal(401, 'invalid_credentials');

		await expect(account.signIn('demo', 'wrong')).rejects.toMatchObject({
			name: 'Server',
			message: 'The handle or the password is wrong.'
		});

		expect(await account.account()).toBeUndefined();
		expect((await browser.stored()).token).toBeUndefined();
	});

	it('asks the server for nothing when there is nothing to send', async () => {
		browser = new Browser();
		const { account } = await browser.tab();

		await expect(account.signIn(' ', '')).rejects.toThrow('Enter your handle and your password.');

		expect(browser.server.asked).toEqual([]);
	});

	it('says so when the server cannot be reached', async () => {
		browser = new Browser();
		const { account } = await browser.tab();
		browser.server.instead = () => {
			const error = new Error('Venti could not be reached.');
			error.name = UNREACHABLE;
			throw error;
		};

		await expect(account.signIn('demo', 'demo-pilot-password')).rejects.toMatchObject({
			name: UNREACHABLE
		});
		expect(await account.account()).toBeUndefined();
	});
});

describe('a new account', () => {
	it('is refused by the core before the server is asked, in words', async () => {
		browser = new Browser();
		const { account } = await browser.tab();
		const wanted = { handle: 'dario', displayName: 'Dario', password: 'too short', invite: '' };

		await expect(account.createAccount(wanted)).rejects.toThrow('at least 15 characters');
		await expect(account.createAccount({ ...wanted, handle: 'ab' })).rejects.toThrow(
			'A handle has at least 3 characters.'
		);

		expect(browser.server.asked).toEqual([]);
	});

	it('is created with the invite code when one was entered, and signs in', async () => {
		browser = new Browser();
		const { account } = await browser.tab();

		await account.createAccount({
			handle: 'Dario_B',
			displayName: ' Dario B. ',
			password: 'correct horse battery staple',
			invite: ' 7k3m-9q2x '
		});

		expect(JSON.parse(browser.server.asked[0].body)).toEqual({
			handle: 'dario_b',
			display_name: 'Dario B.',
			password: 'correct horse battery staple',
			invite: '7k3m-9q2x',
			device: 'Test browser'
		});
		expect(browser.server.asked[0].path).toBe('/v1/accounts');
		expect((await browser.stored()).token).toBe(TOKEN);
	});

	it('hears the server’s refusal of an invite code in the core’s words', async () => {
		browser = new Browser();
		const { account } = await browser.tab();
		browser.server.instead = () => refusal(403, 'invalid_invite');

		await expect(
			account.createAccount({
				handle: 'dario',
				displayName: 'Dario',
				password: 'correct horse battery staple',
				invite: ''
			})
		).rejects.toThrow('The invite code is missing, mistyped, already used or expired.');
	});
});

describe('signing in with Google', () => {
	/** The account as the server says it with the Google account linked. */
	const LINKED = json({
		account: {
			handle: 'demo',
			display_name: 'Demo Pilot',
			created_at: '2026-11-18T19:05:00Z',
			edits_sites: true,
			google_email: 'demo@example.com'
		}
	});

	it('goes on to the handle step for a Google account no account is linked to, storing nothing', async () => {
		browser = new Browser();
		const { account } = await browser.tab();
		browser.server.instead = (request) =>
			request.path === '/v1/auth/google' ? answer(200, recorded('google_sign_up.json')) : undefined;

		const entered = await account.googleSignIn(' google-id-token ');

		expect(entered).toEqual({
			kind: 'needsHandle',
			signUp: {
				signup: 'venti_signup_kZ3x7k9q2m4r8t1v6w0y5a2b3c4d5e6f7g8h9j0',
				suggestedHandle: 'ui_test',
				suggestionLine: expect.stringContaining('@ui_test'),
				name: 'UI Test'
			}
		});
		expect(browser.server.asked).toEqual([
			{
				method: 'POST',
				path: '/v1/auth/google',
				token: undefined,
				body: '{"id_token":"google-id-token","device":"Test browser"}'
			}
		]);
		expect(await account.account()).toBeUndefined();
		expect((await browser.stored()).token).toBeUndefined();
	});

	it('signs in at once for a Google account an account is linked to, the token stored with it', async () => {
		browser = new Browser();
		const { account } = await browser.tab();
		browser.server.instead = (request) =>
			request.path === '/v1/auth/google' ? answer(201, recorded('sign_in.json')) : undefined;

		const entered = await account.googleSignIn('google-id-token');

		expect(entered).toMatchObject({ kind: 'signedIn', account: { handle: 'demo' } });
		expect((await browser.stored()).token).toBe(TOKEN);
	});

	it('creates the account with the handle chosen, and signs in', async () => {
		browser = new Browser();
		const { account } = await browser.tab();
		browser.server.instead = (request) =>
			request.path === '/v1/auth/google/account'
				? answer(201, recorded('sign_in.json'))
				: undefined;

		const signedIn = await account.createGoogleAccount({
			signup: 'venti_signup_x',
			handle: 'ui_test',
			displayName: 'UI Test',
			invite: ''
		});

		expect(signedIn.handle).toBe('demo');
		expect(browser.server.asked).toEqual([
			{
				method: 'POST',
				path: '/v1/auth/google/account',
				token: undefined,
				body: '{"signup":"venti_signup_x","handle":"ui_test","display_name":"UI Test","device":"Test browser"}'
			}
		]);
		expect((await browser.stored()).token).toBe(TOKEN);
	});

	it('refuses a handle the core does not take before the server is asked', async () => {
		browser = new Browser();
		const { account } = await browser.tab();

		await expect(
			account.createGoogleAccount({ signup: 's', handle: 'no', displayName: 'X', invite: '' })
		).rejects.toThrow('A handle has at least 3 characters');
		expect(browser.server.asked).toEqual([]);
	});

	it('says in the core’s words when Google’s token is not good for the server', async () => {
		browser = new Browser();
		const { account } = await browser.tab();
		browser.server.instead = () => refusal(401, 'invalid_google_token');

		await expect(account.googleSignIn('google-id-token')).rejects.toThrow(
			'Google’s sign-in did not work for Venti. Try Continue with Google again.'.replace('’', "'")
		);
		expect((await browser.stored()).token).toBeUndefined();
	});

	it('links and unlinks the account, and remembers the email', async () => {
		browser = new Browser();
		const { account } = await browser.tab();
		await account.signIn('demo', 'demo-pilot-password');
		browser.server.asked = [];
		browser.server.instead = (request) =>
			request.method === 'POST' && request.path === '/v1/account/google'
				? answer(200, LINKED)
				: undefined;

		expect((await account.linkGoogle('google-id-token'))?.googleEmail).toBe('demo@example.com');
		expect((await account.account())?.googleEmail).toBe('demo@example.com');

		expect((await account.unlinkGoogle())?.googleEmail).toBeUndefined();
		expect(browser.server.lines).toEqual(['POST /v1/account/google', 'DELETE /v1/account/google']);
		expect(browser.server.asked.map(({ token }) => token)).toEqual([TOKEN, TOKEN]);
		expect((await account.account())?.googleEmail).toBeUndefined();
	});

	it('keeps an account Google is the only way into linked, and says so', async () => {
		browser = new Browser();
		const { account } = await browser.tab();
		await account.signIn('demo', 'demo-pilot-password');
		browser.server.instead = (request) =>
			request.method === 'DELETE' && request.path === '/v1/account/google'
				? refusal(409, 'google_only_sign_in')
				: undefined;

		await expect(account.unlinkGoogle()).rejects.toThrow(
			'Google is the only way into your account, so it stays linked.'
		);
	});

	it('deletes an account with a fresh token from Google in place of the password', async () => {
		browser = new Browser();
		const { account } = await browser.tab();
		await account.signIn('demo', 'demo-pilot-password');
		browser.server.asked = [];

		await account.deleteAccountWithGoogle('google-id-token');

		expect(browser.server.asked).toEqual([
			{
				method: 'DELETE',
				path: '/v1/account',
				token: TOKEN,
				body: '{"password":"","google_id_token":"google-id-token"}'
			}
		]);
		expect(await account.account()).toBeUndefined();
	});

	it('has the core’s words for its screens', async () => {
		browser = new Browser();
		const { account } = await browser.tab();
		expect(account.googleWords()).toMatchObject({
			continueWithGoogle: 'Continue with Google',
			withPassword: 'Sign in with a handle and password',
			chooseHandle: 'Choose your handle',
			createAccount: 'Create account',
			linkGoogle: 'Link Google',
			unlinkGoogle: 'Unlink Google'
		});
	});
});

describe('the pilot’s own sites, signed in', () => {
	/** The Wallberg's ramp and the Jägerkamp, as the server's recorded map has them. */
	const RAMP = 'osm-free-flying:node/5964676196';
	const JAEGERKAMP = 'osm-free-flying:way/1080986173';

	async function keep(session: CoreSession, id: string): Promise<void> {
		const map = sitesMap(47, 11, 48, 12.5, 200, recorded('sites.json'));
		const site = map.sites.find((one) => one.id === id)!;
		await session.keepSite(site, map.credits, map.asOfUnixS);
	}

	const note = async (session: CoreSession) => (await session.keptSites(undefined)).note;
	const held = async (session: CoreSession) =>
		(await session.keptSites(undefined)).sites.map((kept) => kept.site.id);

	it('stay in the browser alone while it is not signed in: nothing is asked', async () => {
		browser = new Browser();
		const { session, account } = await browser.tab();
		await keep(session, RAMP);

		expect(await account.syncKeptSites()).toBeUndefined();
		expect(browser.server.asked).toEqual([]);
		expect(await note(session)).toMatch(/^Kept on this device only: sign in/);
	});

	it('go to the account with the sync after signing in, and the note says where the list lives', async () => {
		browser = new Browser();
		const { session, account } = await browser.tab();
		await keep(session, RAMP);
		await account.signIn('demo', 'demo-pilot-password');
		expect(await note(session)).toMatch(
			/^Kept on this device, and on your Venti account after the next sync\./
		);
		browser.server.asked = [];

		const outcome = await account.syncKeptSites();

		expect(outcome).toEqual({
			sent: 1,
			fetched: 0,
			removedHere: 0,
			removedThere: 0,
			refused: 0
		});
		// With the session: what the account has, then the offer. No flights:
		// the web keeps no logbook of its own.
		expect(browser.server.lines).toEqual([
			'GET /v1/account/kept-sites/changes',
			`PUT /v1/account/kept-sites/${RAMP}`
		]);
		expect(browser.server.asked.map((asked) => asked.token)).toEqual([TOKEN, TOKEN]);
		expect(JSON.parse(browser.server.asked[1].body).site.id).toBe(RAMP);
		expect(await note(session)).toMatch(/^Kept on your Venti account and on this device\./);

		// Every step was stored: a reload has the same to say and nothing to send.
		browser.closeTabs();
		const reloaded = await browser.tab();
		expect(await note(reloaded.session)).toMatch(
			/^Kept on your Venti account and on this device\./
		);
		browser.server.asked = [];
		expect(await reloaded.account.syncKeptSites()).toMatchObject({ sent: 0, fetched: 0 });
		expect(browser.server.lines).toEqual(['GET /v1/account/kept-sites/changes']);
	});

	it('are sent and taken back one at a time when one is kept or forgotten', async () => {
		browser = new Browser();
		const { session, account } = await browser.tab();
		await account.signIn('demo', 'demo-pilot-password');
		await account.syncKeptSites();

		await keep(session, RAMP);
		browser.server.asked = [];
		expect(await account.syncKeptSites()).toMatchObject({ sent: 1 });
		expect(browser.server.lines).toEqual([
			'GET /v1/account/kept-sites/changes',
			`PUT /v1/account/kept-sites/${RAMP}`
		]);

		await session.forgetSite(RAMP);
		browser.server.asked = [];
		expect(await account.syncKeptSites()).toMatchObject({ removedThere: 1 });
		expect(browser.server.lines[0]).toBe(`DELETE /v1/account/kept-sites/${RAMP}`);
		expect(browser.server.asked[0].token).toBe(TOKEN);
		expect(await held(session)).toEqual([]);
	});

	it('come back from the account in a browser that held none, and stay after a reload', async () => {
		browser = new Browser();
		const { session, account } = await browser.tab();
		// The server's recorded list keeps the Jägerkamp.
		browser.server.keptChanges = answer(200, recorded('kept_site_changes.json'));
		await account.signIn('demo', 'demo-pilot-password');

		const outcome = await account.syncKeptSites();

		expect(outcome).toMatchObject({ sent: 0, fetched: 1 });
		expect(await held(session)).toEqual([JAEGERKAMP]);
		expect(await note(session)).toMatch(/^Kept on your Venti account and on this device\./);

		browser.closeTabs();
		const reloaded = await browser.tab();
		expect(await held(reloaded.session)).toEqual([JAEGERKAMP]);
	});

	it('wait in the browser when the server cannot be reached, and the note says so', async () => {
		browser = new Browser();
		const { session, account } = await browser.tab();
		await account.signIn('demo', 'demo-pilot-password');
		await account.syncKeptSites();
		await keep(session, RAMP);
		browser.server.instead = () => {
			throw unreachableError('Venti could not be reached.');
		};

		expect(await account.syncKeptSites()).toBeUndefined();
		expect(await held(session)).toEqual([RAMP]);
		expect(await note(session)).toMatch(
			/^Kept on this device, and on your Venti account after the next sync\./
		);
	});

	it('are not asked about once the session has ended', async () => {
		browser = new Browser();
		const { account } = await browser.tab();
		await account.signIn('demo', 'demo-pilot-password');
		browser.server.instead = () => refusal(401, 'unauthorized');
		browser.server.asked = [];

		expect(await account.syncKeptSites()).toBeUndefined();
		expect(browser.server.lines).toEqual(['GET /v1/account/kept-sites/changes']);
		// The server no longer takes the token: it is forgotten.
		expect((await browser.stored()).token).toBeUndefined();
	});
});

describe('signing out', () => {
	it('tells the server, and forgets the account and the token', async () => {
		browser = new Browser();
		const { account } = await browser.tab();
		await account.signIn('demo', 'demo-pilot-password');
		browser.server.asked = [];

		await account.signOut();

		expect(browser.server.asked).toEqual([
			{ method: 'DELETE', path: '/v1/sessions/3kq9x2m7r4t8v1w6y0a5b3c2d4', token: TOKEN, body: '' }
		]);
		expect(await account.account()).toBeUndefined();
		const stored = await browser.stored();
		expect(stored.token).toBeUndefined();
		expect(stored.names).not.toContain('account.json');
	});

	it('forgets them also when the server cannot be reached', async () => {
		browser = new Browser();
		const { account } = await browser.tab();
		await account.signIn('demo', 'demo-pilot-password');
		browser.server.instead = () => {
			throw new Error('offline');
		};

		await account.signOut();

		expect(await account.account()).toBeUndefined();
		expect((await browser.stored()).token).toBeUndefined();
	});

	it('signs the site’s other tabs out with it', async () => {
		browser = new Browser();
		const first = await browser.tab();
		const second = await browser.tab();
		await first.account.signIn('demo', 'demo-pilot-password');
		await second.session.reload();
		expect((await second.account.account())?.handle).toBe('demo');

		await first.account.signOut();
		await second.session.reload();

		expect(await second.account.account()).toBeUndefined();
	});
});

describe('deleting the account', () => {
	it('needs the password, and keeps the account when it is wrong', async () => {
		browser = new Browser();
		const { account } = await browser.tab();
		await account.signIn('demo', 'demo-pilot-password');
		browser.server.instead = (request) =>
			request.path === '/v1/account' ? refusal(403, 'wrong_password') : undefined;

		await expect(account.deleteAccount('wrong')).rejects.toThrow('The password is wrong.');

		expect((await account.account())?.handle).toBe('demo');
		expect((await browser.stored()).token).toBe(TOKEN);
	});

	it('forgets the account here', async () => {
		browser = new Browser();
		const { account } = await browser.tab();
		await account.signIn('demo', 'demo-pilot-password');
		browser.server.asked = [];

		await account.deleteAccount('demo-pilot-password');

		expect(browser.server.asked).toEqual([
			{
				method: 'DELETE',
				path: '/v1/account',
				token: TOKEN,
				body: '{"password":"demo-pilot-password"}'
			}
		]);
		expect(await account.account()).toBeUndefined();
		expect((await browser.stored()).token).toBeUndefined();
	});
});

describe('reporting what another pilot shares', () => {
	/** The flight of the server's recorded report. */
	const REPORTED = { kind: 'flight', id: 'fbyw03atd9qdt5bfe177ra1k94' } as const;
	const WORDS = 'I flew there that day. This is my track, not hers.';

	/** A signed-in tab, with what the server has been asked so far forgotten. */
	async function signedIn() {
		browser = new Browser();
		const { account } = await browser.tab();
		await account.signIn('demo', 'demo-pilot-password');
		browser.server.asked = [];
		return account;
	}

	it('offers the server’s six reasons, each with a line, and says what happens to a report', async () => {
		browser = new Browser();
		const { account } = await browser.tab();

		const form = account.reportForm({ kind: 'flight', id: 'x' });

		expect(form.reasons.map((choice) => choice.reason)).toEqual([
			'offensive',
			'impersonation',
			'privacy',
			'notAFlight',
			'spam',
			'other'
		]);
		expect(form.reasons.every((choice) => choice.title && choice.explanation)).toBe(true);
		expect(form.textAtMost).toBe(1000);
		// A report is not anonymous to whoever runs the server, and says so.
		expect(form.notice).toContain('sees that it came from you');
	});

	it('offers a group and a plan reasons of their own', async () => {
		browser = new Browser();
		const { account } = await browser.tab();

		const flight = account.reportForm({ kind: 'flight', id: 'x' });
		const group = account.reportForm({ kind: 'group', id: 'g' });
		const plan = account.reportForm({ kind: 'plan', groupId: 'g', id: 'p' });

		expect(group.reasons).toHaveLength(5);
		expect(plan.reasons).toHaveLength(5);
		expect(group.reasons.map((choice) => choice.reason)).not.toContain('notAFlight');
		expect(group.reasons.every((choice) => choice.title && choice.explanation)).toBe(true);
		expect(group.notice).not.toEqual('');
		expect(group.reasons).not.toEqual(flight.reasons);
	});

	it('goes to the server from the pilot’s account, as the server’s own example writes it', async () => {
		const account = await signedIn();
		browser.server.instead = () => answer(201, recorded('report.json'));

		const said = await account.report(REPORTED, 'notAFlight', WORDS);

		expect(browser.server.asked).toHaveLength(1);
		const [asked] = browser.server.asked;
		expect(asked).toMatchObject({ method: 'POST', path: '/v1/reports', token: TOKEN });
		expect(JSON.parse(asked.body)).toEqual(JSON.parse(text(recorded('report_request.json'))));
		expect(said).toBe('Sent. Whoever runs Venti will read it.');
	});

	it('names a pilot’s page and a live track as the server wants them named', async () => {
		const account = await signedIn();
		browser.server.instead = () => answer(201, recorded('report.json'));

		await account.report({ kind: 'pilot', handle: 'mara' }, 'impersonation', '');
		await account.report({ kind: 'liveTrack', id: '11111111111111111111111111' }, 'privacy', '');

		expect(browser.server.asked.map((asked) => JSON.parse(asked.body))).toEqual([
			{ subject: { pilot: 'mara' }, reason: 'impersonation' },
			{ subject: { live_track: '11111111111111111111111111' }, reason: 'privacy' }
		]);
	});

	it('says that a thing reported before is still waiting to be read', async () => {
		const account = await signedIn();
		browser.server.instead = () => answer(200, json({ created: false }));

		expect(await account.report(REPORTED, 'spam', '')).toBe(
			'You reported this before. Your report is still waiting to be read.'
		);
	});

	it('is refused by the core before the server is asked, where words are missing or too many', async () => {
		const account = await signedIn();

		await expect(account.report(REPORTED, 'other', '  ')).rejects.toThrow(
			'Say in a few words what is wrong.'
		);
		await expect(account.report(REPORTED, 'spam', 'x'.repeat(1001))).rejects.toThrow(
			'A report has room for 1000 characters. Shorten it.'
		);
		expect(browser.server.asked).toEqual([]);
	});

	it('hears the server’s refusals in the core’s words', async () => {
		const account = await signedIn();

		browser.server.instead = () => refusal(404, 'flight_not_found');
		await expect(account.report(REPORTED, 'spam', '')).rejects.toThrow(
			'This flight is no longer there.'
		);
		browser.server.instead = () => refusal(429, 'rate_limited');
		await expect(account.report(REPORTED, 'spam', '')).rejects.toThrow(
			'You have sent many reports today. Try again tomorrow.'
		);
	});

	it('is told by the server to sign in when this browser is not signed in', async () => {
		browser = new Browser();
		const { account } = await browser.tab();
		browser.server.instead = () => refusal(401, 'unauthorized');

		await expect(account.report(REPORTED, 'spam', '')).rejects.toThrow(
			'This device is no longer signed in. Sign in again.'
		);
		// No token was there to send.
		expect(browser.server.asked[0].token).toBeUndefined();
	});
});

describe('what a browser is called among the account’s devices', () => {
	it('is its name and the system it runs on, as far as it says', () => {
		const said = (agent: string) => deviceName(agent);
		expect(said('Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0')).toBe(
			'Firefox on Linux'
		);
		expect(
			said(
				'Mozilla/5.0 (iPhone; CPU iPhone OS 26_1 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.1 Mobile/15E148 Safari/604.1'
			)
		).toBe('Safari on iPhone');
		expect(
			said(
				'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36 Edg/140.0.0.0'
			)
		).toBe('Edge on Windows');
		expect(
			said(
				'Mozilla/5.0 (Linux; Android 16; Pixel 9) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Mobile Safari/537.36'
			)
		).toBe('Chrome on Android');
		expect(
			said(
				'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15'
			)
		).toBe('Safari on Mac');
		expect(said('something else')).toBe('Web browser');
	});
});

describe('how long a server asks to wait', () => {
	it('is read when given in seconds, and passed over otherwise', () => {
		expect(wholeSeconds('30')).toBe(30);
		expect(wholeSeconds(' 120 ')).toBe(120);
		expect(wholeSeconds('Wed, 21 Oct 2026 07:28:00 GMT')).toBeUndefined();
		expect(wholeSeconds('-5')).toBeUndefined();
		expect(wholeSeconds(null)).toBeUndefined();
	});
});
