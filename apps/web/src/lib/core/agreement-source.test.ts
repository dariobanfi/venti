import { readFileSync } from 'node:fs';
import { afterEach, beforeAll, describe, expect, it } from 'vitest';
import { AccountSource, type Send, type ServerAnswer } from './account-source';
import { AgreementSource } from './agreement-source';
import type { ServerRequest } from './pkg/venti_core_wasm.js';
import { unreachable } from './protocol';
import { CoreSession } from './session';
import { MemoryStore } from './store';
import { startCore } from './testing';

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
/** An identifier as the server makes them; nobody was given this one. */
const ID = `venti_oar_${'0'.repeat(52)}`;
const PATH = `/v1/oauth/requests/${ID}`;
/** 18 November 2026, 19:05 UTC: a quarter of an hour before the recorded request ends. */
const NOW = Date.UTC(2026, 10, 18, 19, 5) / 1000;

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
	down = false;

	send: Send = async (request, token) => {
		this.asked.push({
			method: request.method,
			path: request.path,
			token: request.needsToken ? token : undefined,
			body: text(request.body)
		});
		if (this.down) throw unreachable('Venti could not be reached.');
		const other = this.instead(request);
		if (other) return other;
		const { method, path } = request;
		if (method === 'POST' && path === '/v1/sessions') return answer(201, recorded('sign_in.json'));
		if (method === 'GET' && path === PATH) return answer(200, recorded('oauth_request.json'));
		if (method === 'POST' && path === PATH) {
			return answer(200, recorded('oauth_request_answered.json'));
		}
		return refusal(404, 'not_found');
	};
}

let session: CoreSession;
afterEach(() => session?.close());

/** A browser, signed in or not, and its way to a program's request. */
async function browser(signedIn: boolean): Promise<{ server: Server; agreement: AgreementSource }> {
	const server = new Server();
	session = await CoreSession.open(new MemoryStore());
	if (signedIn) {
		const account = new AccountSource(session, server.send, 'https://venti.example', 'Test');
		await account.signIn('demo', 'demo-pilot-password');
		server.asked = [];
	}
	return { server, agreement: new AgreementSource(session, async () => NOW, server.send) };
}

describe('a program’s request, as the worker reads it', () => {
	it('has the core’s words for the buttons and for a request that has ended', async () => {
		const { agreement } = await browser(false);
		const { words, ended } = agreement.said();
		expect(words.allow).toBe('Allow');
		expect(words.refuse).toBe("Don't allow");
		expect(words.signIn).toBe('Sign in');
		expect(ended.state).toBe('ended');
		expect(ended.title).toBe('This request has ended');
	});

	it('sends nothing for text that is no identifier, and shows the request as ended', async () => {
		const { server, agreement } = await browser(true);
		for (const bad of ['', 'venti_oar_', 'venti_oar_../../account', `${ID}0`, 'anything else']) {
			const read = await agreement.read(bad);
			expect(read.page, bad).toEqual(agreement.said().ended);
			const outcome = await agreement.answer(bad, true);
			expect(outcome.redirectTo, bad).toBeUndefined();
			expect(outcome.state, bad).toBe('ended');
		}
		expect(server.asked).toEqual([]);
	});

	it('reads the request without a token, for the pilot who is signed in', async () => {
		const { server, agreement } = await browser(true);
		const { page, nowUnixS } = await agreement.read(ID);
		expect(server.asked).toEqual([{ method: 'GET', path: PATH, token: undefined, body: '' }]);
		expect(page.state).toBe('asking');
		expect(page.title).toBe('Let this program read your flights?');
		expect(page.message).toContain('“Assistant”');
		expect(page.account).toBe('You answer as @demo.');
		expect(page.rows.map((row) => row.label)).toEqual([
			'It may not',
			'You are sent back to',
			'For how long'
		]);
		expect(page.endsUnixS - nowUnixS).toBe(15 * 60);
	});

	it('asks a browser that is signed in to nothing to sign in first', async () => {
		const { agreement } = await browser(false);
		const { page } = await agreement.read(ID);
		expect(page.state).toBe('signInFirst');
		expect(page.account).toBeUndefined();
		expect(page.mayRead).toHaveLength(1);
	});

	it('says what became of a request that no longer waits', async () => {
		const { server, agreement } = await browser(true);
		server.instead = () => refusal(404, 'authorization_request_not_found');
		expect((await agreement.read(ID)).page.state).toBe('ended');
		server.instead = () => refusal(410, 'authorization_request_answered');
		expect((await agreement.read(ID)).page.state).toBe('answered');
		server.instead = () => answer(503);
		expect((await agreement.read(ID)).page.state).toBe('failed');
	});

	it('shows a server out of reach as a page that can be tried again', async () => {
		const { server, agreement } = await browser(true);
		server.down = true;
		const { page } = await agreement.read(ID);
		expect(page.state).toBe('failed');
		expect(page.title).toBe('Venti could not be asked');
		expect(page.message).toBe('Venti could not be reached.');
		// The request still waits: the pilot answers again.
		const outcome = await agreement.answer(ID, true);
		expect(outcome).toMatchObject({ state: 'asking', message: 'Venti could not be reached.' });
		expect(outcome.redirectTo).toBeUndefined();
	});

	it('answers with the session’s token and hands on where the browser goes', async () => {
		const { server, agreement } = await browser(true);
		const outcome = await agreement.answer(ID, true);
		expect(server.asked).toEqual([
			{ method: 'POST', path: PATH, token: SESSION, body: '{"agree":true}' }
		]);
		expect(outcome.redirectTo).toMatch(/^https:\/\/assistant\.example\/oauth\/callback\?code=/);
		expect(outcome.message).toBe('');

		await agreement.answer(ID, false);
		expect(server.asked[1].body).toBe('{"agree":false}');
	});

	it('says in the core’s words why an answer was not taken', async () => {
		const { server, agreement } = await browser(true);
		server.instead = () => refusal(409, 'too_many_tokens');
		expect(await agreement.answer(ID, true)).toMatchObject({ state: 'asking' });
		server.instead = () => refusal(401, 'unauthorized');
		expect((await agreement.answer(ID, true)).state).toBe('signInFirst');
		server.instead = () => refusal(410, 'authorization_request_answered');
		expect(await agreement.answer(ID, true)).toMatchObject({
			state: 'answered',
			title: 'Already answered'
		});
		// An address a browser is not sent to.
		server.instead = () => answer(200, json({ redirect_to: 'javascript:alert(1)' }));
		const odd = await agreement.answer(ID, true);
		expect(odd.redirectTo).toBeUndefined();
		expect(odd.state).toBe('failed');
	});
});
