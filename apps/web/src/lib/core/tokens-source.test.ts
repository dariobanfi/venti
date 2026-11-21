import { readFileSync } from 'node:fs';
import { afterEach, beforeAll, describe, expect, it } from 'vitest';
import { AccountSource, type Send, type ServerAnswer } from './account-source';
import type { ServerRequest } from './pkg/venti_core_wasm.js';
import { isInWords } from './index';
import { unreachable } from './protocol';
import { CoreSession } from './session';
import { MemoryStore } from './store';
import { startCore } from './testing';
import { TokensSource } from './tokens-source';

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
const SESSION = 'venti_3x7k9q2m4r8t1v6w0y5a2b3c4d5e6f7g8h9j0kmnpqrstvwxyz01';

/** 21 November 2026, 12:00 UTC, in milliseconds. */
const NOW = Date.UTC(2026, 10, 21, 12);

function answer(status: number, body: Uint8Array = new Uint8Array()): ServerAnswer {
	return { status, body, retryAfterS: undefined };
}

function refusal(status: number, code: string): ServerAnswer {
	return answer(status, json({ error: { code, message: 'for developers' } }));
}

/** A Venti server played from the recorded answers. It notes what it was asked, and with which token. */
class Server {
	asked: { method: string; path: string; token: string | undefined; body: string }[] = [];
	instead: (request: ServerRequest) => ServerAnswer | undefined = () => undefined;

	send: Send = async (request, token) => {
		this.asked.push({
			method: request.method,
			path: request.path,
			token: request.needsToken ? token : undefined,
			body: text(request.body)
		});
		const other = this.instead(request);
		if (other) return other;
		const { method, path } = request;
		if (method === 'POST' && path === '/v1/sessions') return answer(201, recorded('sign_in.json'));
		if (method === 'GET' && path === '/v1/account/tokens') {
			return answer(200, recorded('tokens.json'));
		}
		if (method === 'POST' && path === '/v1/account/tokens') {
			return answer(201, recorded('token_created.json'));
		}
		if (method === 'DELETE' && path.startsWith('/v1/account/tokens/')) return answer(204);
		return refusal(404, 'not_found');
	};
}

let session: CoreSession;
afterEach(() => session?.close());

/** A browser that is signed in, and the way to its account's tokens. */
async function signedIn(): Promise<{ server: Server; tokens: TokensSource }> {
	const server = new Server();
	session = await CoreSession.open(new MemoryStore());
	const account = new AccountSource(
		session,
		server.send,
		'https://venti.example',
		'Test',
		() => NOW
	);
	await account.signIn('demo', 'demo-pilot-password');
	server.asked = [];
	return { server, tokens: new TokensSource(session, server.send, () => NOW) };
}

/** What a call threw, which has to be in words for the pilot. */
async function refused(call: () => unknown): Promise<string> {
	try {
		await call();
	} catch (error) {
		expect(isInWords(error)).toBe(true);
		return (error as Error).message;
	}
	throw new Error('It was not refused.');
}

describe('programs with access', () => {
	it('has the core’s words for the screen and the form, with the twelve things to allow', async () => {
		const { tokens } = await signedIn();
		const form = tokens.form();
		expect(form.title).toBe('Programs with access');
		expect(form.submit).toBe('Give access');
		expect(form.scopes).toHaveLength(12);
		expect(form.scopes[1]).toEqual({
			scope: 'flights:read',
			title: 'Flights: read',
			line: 'your logbook, the private flights too'
		});
		expect(form.empty).toBe('No program has access to your account.');
	});

	it('checks a name by the core before anything is sent', async () => {
		const { server, tokens } = await signedIn();
		expect(tokens.checkName('  My logger ')).toBe('My logger');
		expect(await refused(() => tokens.checkName('   '))).toMatch(/^Give the program a name/);
		expect(server.asked).toEqual([]);
	});

	it('lists them from the account, with the session, each with the words that ask before a withdrawal', async () => {
		const { server, tokens } = await signedIn();
		const listed = await tokens.list();
		expect(server.asked).toEqual([
			{ method: 'GET', path: '/v1/account/tokens', token: SESSION, body: '' }
		]);
		expect(listed).toHaveLength(1);
		expect(listed[0].token).toEqual({
			id: '00000000000000000000000003',
			name: 'Demo logger',
			scopes: ['Flights: read', 'Flights: write'],
			scopesLine: 'Flights: read · Flights: write',
			made: '18 Nov 2026',
			lastUsed: 'never',
			ends: 'never',
			hasEnded: false
		});
		expect(listed[0].withdrawal.title).toBe('Withdraw the access of “Demo logger”?');
		expect(listed[0].withdrawal.confirm).toBe('Withdraw access');
		expect(listed[0].withdrawal.cancel).toBe('Keep access');
	});

	it('marks one whose end has come, by the clock it is given', async () => {
		const { server, tokens } = await signedIn();
		const one = JSON.parse(text(recorded('tokens.json'))).tokens[0];
		server.instead = () =>
			answer(200, json({ tokens: [{ ...one, expires_at: '2026-11-20T23:59:59Z' }] }));
		const [listed] = await tokens.list();
		expect(listed.token.hasEnded).toBe(true);
		expect(listed.token.ends).toBe('on 20 Nov 2026');
	});

	it('gives access as the server’s own example writes it, and hands the secret on once', async () => {
		const { server, tokens } = await signedIn();
		const created = await tokens.give({
			name: ' My logger ',
			scopes: ['live:write', 'flights:write'],
			endsUnixS: undefined,
			password: 'demo-pilot-password'
		});
		expect(server.asked).toHaveLength(1);
		expect(server.asked[0]).toMatchObject({
			method: 'POST',
			path: '/v1/account/tokens',
			token: SESSION
		});
		expect(JSON.parse(server.asked[0].body)).toEqual(
			JSON.parse(text(recorded('token_create_request.json')))
		);
		expect(created.secret).toBe(JSON.parse(text(recorded('token_created.json'))).secret);
		expect(created.token.name).toBe('My logger');
		expect(created.token.scopesLine).toBe('Flights: write · Live tracking: send');
		expect(created.shownOnce).toBe('Copy it now: Venti does not show it again.');
		expect(created.copy).toBe('Copy');
		expect(created.done).toBe('I have copied it');
	});

	it('sends the end the pilot chose', async () => {
		const { server, tokens } = await signedIn();
		await tokens.give({
			name: 'My logger',
			scopes: ['flights:read'],
			endsUnixS: Date.UTC(2026, 11, 21, 23, 59, 59) / 1000,
			password: 'demo-pilot-password'
		});
		expect(JSON.parse(server.asked[0].body).expires_at).toBe('2026-12-21T23:59:59Z');
	});

	it('is refused by the core before the server is asked, in words', async () => {
		const { server, tokens } = await signedIn();
		const wanted = {
			name: 'My logger',
			scopes: ['flights:read'],
			endsUnixS: undefined,
			password: 'demo-pilot-password'
		};
		expect(await refused(() => tokens.give({ ...wanted, name: '' }))).toMatch(
			/^Give the program a name/
		);
		expect(await refused(() => tokens.give({ ...wanted, scopes: [] }))).toBe(
			'Choose at least one thing the program may do.'
		);
		expect(await refused(() => tokens.give({ ...wanted, password: '' }))).toBe(
			'Enter your password.'
		);
		expect(server.asked).toEqual([]);
	});

	it('hears the server’s refusal of a wrong password in the core’s words', async () => {
		const { server, tokens } = await signedIn();
		server.instead = () => refusal(403, 'wrong_password');
		const said = await refused(() =>
			tokens.give({
				name: 'My logger',
				scopes: ['flights:read'],
				endsUnixS: undefined,
				password: 'not the password'
			})
		);
		expect(said).not.toContain('for developers');
		expect(said.length).toBeGreaterThan(0);
	});

	it('withdraws one by its identifier, with the session', async () => {
		const { server, tokens } = await signedIn();
		await tokens.withdraw('00000000000000000000000003');
		expect(server.asked).toEqual([
			{
				method: 'DELETE',
				path: '/v1/account/tokens/00000000000000000000000003',
				token: SESSION,
				body: ''
			}
		]);
		// One that was withdrawn already is withdrawn.
		server.instead = () => refusal(404, 'not_found');
		await tokens.withdraw('00000000000000000000000003');
	});

	it('says so when the server cannot be reached', async () => {
		const { server, tokens } = await signedIn();
		server.send = async () => {
			throw unreachable('Venti could not be reached.');
		};
		const offline = new TokensSource(session, server.send, () => NOW);
		void tokens;
		expect(await refused(() => offline.list())).toBe('Venti could not be reached.');
	});
});
