import { readFileSync } from 'node:fs';
import { afterEach, beforeAll, describe, expect, it } from 'vitest';
import { AccountSource, type Send, type ServerAnswer } from './account-source';
import { AlertsSource } from './alerts-source';
import type { AlertDraft, ServerRequest } from './pkg/venti_core_wasm.js';
import { isInWords } from './index';
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

/** 21 November 2026, 12:00 UTC, in seconds. */
const NOW_S = Date.UTC(2026, 10, 21, 12) / 1000;

/** Central European Time in winter. */
const CET = 3600;

function answer(status: number, body: Uint8Array = new Uint8Array()): ServerAnswer {
	return { status, body, retryAfterS: undefined };
}

/** A Venti server played from the recorded answers. It notes what it was asked, and with which token. */
class Server {
	asked: { method: string; path: string; token: string | undefined; body: string }[] = [];
	fired = recorded('alerts_fired.json');
	/** Answers a request differently, if it gives an answer. */
	instead: (request: ServerRequest) => ServerAnswer | undefined = () => undefined;

	send: Send = async (request: ServerRequest, token) => {
		this.asked.push({
			method: request.method,
			path: request.path,
			token: request.needsToken ? token : undefined,
			body: text(request.body)
		});
		const { method, path } = request;
		const other = this.instead(request);
		if (other) return other;
		if (method === 'POST' && path === '/v1/sessions') return answer(201, recorded('sign_in.json'));
		if (method === 'GET' && path === '/v1/account/alerts')
			return answer(200, recorded('alerts.json'));
		if (method === 'GET' && path.startsWith('/v1/account/alerts/fired')) {
			return answer(200, this.fired);
		}
		if (method === 'POST' && path === '/v1/account/alerts')
			return answer(201, recorded('alert.json'));
		if (method === 'PUT' && path.startsWith('/v1/account/alerts/')) {
			return answer(200, recorded('alert.json'));
		}
		if (method === 'DELETE' && path.startsWith('/v1/account/alerts/')) return answer(204);
		return answer(404, json({ error: { code: 'not_found', message: 'for developers' } }));
	};
}

let session: CoreSession;
afterEach(() => session?.close());

async function signedIn(): Promise<{ server: Server; alerts: AlertsSource }> {
	const server = new Server();
	session = await CoreSession.open(new MemoryStore());
	const account = new AccountSource(
		session,
		server.send,
		'https://venti.example',
		'Test',
		() => NOW_S * 1000
	);
	await account.signIn('demo', 'demo-pilot-password');
	server.asked = [];
	return { server, alerts: new AlertsSource(session, server.send) };
}

/** A wind alarm as the form starts it on a station's page. */
const wind: AlertDraft = {
	name: 'Patscherkofel',
	enabled: true,
	kind: 'wind',
	stationId: 'geosphere-tawes:11126',
	stationName: 'Patscherkofel',
	fromPoint: undefined,
	toPoint: undefined,
	minKmh: undefined,
	maxKmh: undefined,
	gustsMaxKmh: undefined,
	fromHour: undefined,
	toHour: undefined,
	lat: 0,
	lon: 0,
	siteId: undefined,
	placeName: '',
	daysAhead: [],
	atLeast: 'crossCountry'
};

describe('alarms', () => {
	it('has the sixteen points, and the core’s line and faults for a draft', async () => {
		const { alerts, server } = await signedIn();
		expect(alerts.points()).toHaveLength(16);
		expect(alerts.points()[2]).toBe('NE');
		const faults = alerts.check(wind);
		expect(faults.map((fault) => fault.field)).toEqual(['rule.directions']);
		expect(faults[0].message).toMatch(/^The directions: /);
		const fixed = { ...wind, fromPoint: 0, toPoint: 2, minKmh: 10, maxKmh: 20 };
		expect(alerts.check(fixed)).toEqual([]);
		expect(alerts.line(fixed)).toBe('Wind at Patscherkofel from N to NE, 10 to 20 km/h');
		expect(server.asked).toEqual([]);
	});

	it('holds nothing before the first run, and the list and what fired after it', async () => {
		const { alerts, server } = await signedIn();
		expect(await alerts.held(NOW_S, CET)).toEqual({ alerts: [], fired: [] });

		const synced = await alerts.sync(CET);
		expect(server.asked.map(({ method, path }) => `${method} ${path}`)).toEqual([
			'GET /v1/account/alerts',
			expect.stringMatching(/^GET \/v1\/account\/alerts\/fired/)
		]);
		expect(server.asked.every(({ token }) => token === SESSION)).toBe(true);
		expect(synced).toMatchObject({ kind: 'finished' });
		const held = await alerts.held(NOW_S, CET);
		expect(held.alerts.map((alert) => alert.name)).toEqual([
			'Patscherkofel north',
			'Nordkette cross-country'
		]);
		expect(held.alerts[0].line).toBe(
			'Wind at Patscherkofel from N to NE, 10 to 20 km/h, gusts at most 30, between 10:00 and 16:00'
		);
		expect(held.alerts[0].draft?.fromPoint).toBe(0);
		expect(held.fired).toHaveLength(1);
		expect(held.fired[0].line).toMatch(
			/^Patscherkofel: 15 km\/h from the north, gusts 22, at 11:20/
		);
		if (synced?.kind === 'finished')
			expect(synced.newLines.map((one) => one.line)).toEqual([held.fired[0].line]);
	});

	it('says nothing is new on the next run, and asks for what fired since the cursor', async () => {
		const { alerts, server } = await signedIn();
		await alerts.sync(CET);
		server.asked = [];
		server.fired = json({ fired: [], cursor: '1794997380000000', more: false, reset: false });
		const again = await alerts.sync(CET);
		expect(again).toEqual({ kind: 'finished', newLines: [] });
		expect(server.asked[1].path).toContain('since=1794997380000000');
		expect((await alerts.held(NOW_S, CET)).fired).toHaveLength(1);
	});

	it('asks nothing in a browser that is not signed in', async () => {
		const server = new Server();
		session = await CoreSession.open(new MemoryStore());
		const alerts = new AlertsSource(session, server.send);
		expect(await alerts.sync(CET)).toBeUndefined();
		expect(await alerts.held(NOW_S, CET)).toEqual({ alerts: [], fired: [] });
		expect(server.asked).toEqual([]);
	});

	it('adds, changes and removes an alarm with the session', async () => {
		const { alerts, server } = await signedIn();
		const draft = { ...wind, name: 'Patscherkofel north', fromPoint: 0, toPoint: 2, minKmh: 10 };
		const added = await alerts.add(draft);
		expect(added.name).toBe('Patscherkofel north');
		expect(server.asked[0]).toMatchObject({
			method: 'POST',
			path: '/v1/account/alerts',
			token: SESSION
		});
		expect(JSON.parse(server.asked[0].body)).toMatchObject({
			name: 'Patscherkofel north',
			rule: { kind: 'wind', station: 'geosphere-tawes:11126', min_kmh: 10 }
		});
		await alerts.change(added.id, { ...draft, enabled: false });
		expect(server.asked[1]).toMatchObject({
			method: 'PUT',
			path: `/v1/account/alerts/${added.id}`
		});
		expect(JSON.parse(server.asked[1].body).enabled).toBe(false);
		await alerts.remove(added.id);
		expect(server.asked[2]).toMatchObject({
			method: 'DELETE',
			path: `/v1/account/alerts/${added.id}`
		});
	});

	it('has every word of the screens from the core, the days and kinds in the draft’s order', async () => {
		const { alerts } = await signedIn();
		const words = alerts.formWords();
		expect(words).toMatchObject({
			alertMe: 'Alert me',
			changeTitle: 'Change the alarm',
			title: 'Alerts',
			least: 'Least wind',
			hoursTitle: 'Hours, Central European Time',
			signedOut: 'Alarms live on your account: sign in to set one.',
			delete: 'Delete alarm'
		});
		expect(words.dayNames).toEqual(['Today', 'Tomorrow', 'In 2 days', 'In 3 days']);
		expect(words.dayKinds.map((choice) => choice.kind)).toEqual([
			'flyable',
			'local',
			'crossCountry',
			'big'
		]);
		expect(words.fields.find((one) => one.field === 'rule.min_kmh')?.name).toBe('The least wind');
	});

	it('takes an alarm that is gone already for removed, and says a refusal in the core’s words', async () => {
		const { alerts, server } = await signedIn();
		server.instead = () => answer(404, json({ error: { code: 'not_found', message: 'gone' } }));
		await expect(alerts.remove('a1')).resolves.toBeUndefined();
		server.instead = () =>
			answer(401, json({ error: { code: 'unauthorized', message: 'for developers' } }));
		await expect(alerts.remove('a1')).rejects.toSatisfy(isInWords);
	});

	it('is refused by the core, in words, for a draft without a station', async () => {
		const { alerts, server } = await signedIn();
		let said = '';
		try {
			await alerts.add({ ...wind, stationId: '' });
		} catch (error) {
			expect(isInWords(error)).toBe(true);
			said = (error as Error).message;
		}
		expect(said).toBe('Choose a station.');
		expect(server.asked).toEqual([]);
	});
});
