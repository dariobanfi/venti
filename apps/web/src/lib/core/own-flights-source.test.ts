import { readFileSync } from 'node:fs';
import { beforeAll, describe, expect, it } from 'vitest';
import type { ServerAnswer } from './account-source';
import { OwnFlightsSource } from './own-flights-source';
import type { ServerRequest } from './pkg/venti_core_wasm.js';
import { sampleFlight, startCore } from './testing';

beforeAll(startCore);

/** What the development server answered, as its tests recorded it. */
function recorded(name: string): Uint8Array {
	return readFileSync(
		new URL(`../../../../../crates/server/tests/golden/${name}`, import.meta.url)
	);
}

const TOKEN = 'venti_test';

/** A server played from its recorded answers, which notes what it was asked and with which token. */
function played(answers: Record<string, () => ServerAnswer>) {
	const asked: { line: string; token: string | undefined; body?: string }[] = [];
	const send = async (request: ServerRequest, token: string | undefined) => {
		const line = `${request.method} ${request.path}`;
		asked.push({
			line,
			token,
			body: request.body && new TextDecoder().decode(request.body)
		});
		const answer = answers[line];
		if (!answer) throw new Error(`Not played: ${line}`);
		return answer();
	};
	return { asked, send };
}

const ok = (body: Uint8Array, status = 200): ServerAnswer => ({
	status,
	body,
	retryAfterS: undefined
});

describe('the pilot’s own flights on the server', () => {
	it('are listed with the session, read by the core, each with who may see it', async () => {
		const server = played({ 'GET /v1/flights?limit=100': () => ok(recorded('flights.json')) });
		const source = new OwnFlightsSource(async () => TOKEN, server.send, 'https://venti.example');

		const page = await source.list();

		expect(page?.next).toBeUndefined();
		expect(page?.flights.map((flight) => flight.sharing.visibility)).toEqual([
			'public',
			'unlisted',
			'private'
		]);
		const [first] = page!.flights;
		expect(first.row).toMatchObject({ id: '5t1eshypdtvrq7rxw39a18vsrg', glider: 'Sample Wing' });
		expect(first.sharing).toEqual({
			visibility: 'public',
			link: 'https://venti.example/f/5t1eshypdtvrq7rxw39a18vsrg',
			flightId: '5t1eshypdtvrq7rxw39a18vsrg'
		});
		expect(server.asked).toEqual([{ line: 'GET /v1/flights?limit=100', token: TOKEN }]);
	});

	it('are not asked for in a browser that is not signed in', async () => {
		const server = played({});
		const source = new OwnFlightsSource(
			async () => undefined,
			server.send,
			'https://venti.example'
		);

		expect(await source.list()).toBeUndefined();
		expect(await source.open('5t1eshypdtvrq7rxw39a18vsrg')).toBeUndefined();
		expect(await source.upload(new ArrayBuffer(0))).toBeUndefined();
		expect(await source.statistics()).toBeUndefined();
		expect(server.asked).toEqual([]);
	});

	it('open one from its file, with what happened in it', async () => {
		const id = '5t1eshypdtvrq7rxw39a18vsrg';
		const server = played({
			[`GET /v1/flights/${id}`]: () => ok(recorded('flight.json')),
			[`GET /v1/flights/${id}/igc`]: () => ok(new TextEncoder().encode(sampleFlight()))
		});
		const source = new OwnFlightsSource(async () => TOKEN, server.send, 'https://venti.example');

		const opened = await source.open(id);

		expect(opened?.flight.track).toHaveLength(1745);
		expect(opened?.analysis?.thermals.length).toBeGreaterThan(0);
		expect(opened?.sharing.visibility).toBe('public');
		expect(server.asked.map((asked) => asked.token)).toEqual([TOKEN, TOKEN]);
		// The name the logbook gives it, from the file itself.
		expect(opened?.name).toBe('2026-07-04-XVE-000-01.igc');
		expect(opened?.text).toBe(sampleFlight());
	});

	it('take a file in as an upload, and say whether the account had it', async () => {
		const server = played({ 'POST /v1/flights': () => ok(recorded('upload.json'), 201) });
		const source = new OwnFlightsSource(async () => TOKEN, server.send, 'https://venti.example');
		const bytes = new TextEncoder().encode(sampleFlight());

		const uploaded = await source.upload(bytes.buffer as ArrayBuffer);

		expect(uploaded?.alreadyThere).toBe(false);
		expect(uploaded?.flight.sharing.visibility).toBe('public');
		expect(server.asked[0]).toMatchObject({ token: TOKEN, body: sampleFlight() });
	});

	it('say of a flight that was taken down that it was', async () => {
		const id = '5t1eshypdtvrq7rxw39a18vsrg';
		const server = played({
			[`PATCH /v1/flights/${id}`]: () => ok(recorded('flight_taken_down.json'))
		});
		const source = new OwnFlightsSource(async () => TOKEN, server.send, 'https://venti.example');

		const flight = await source.setGliderClass(id, 'b');

		expect(flight.takenDown).toBe(true);
		expect(flight.sharing.visibility).toBe('private');
		expect(JSON.parse(server.asked[0].body!)).toMatchObject({ glider_class: expect.anything() });
	});

	it('add up all the account’s flights, the private ones too, as a pilot’s statistics', async () => {
		const server = played({
			'GET /v1/account/statistics': () => ok(recorded('account_statistics.json'))
		});
		const source = new OwnFlightsSource(async () => TOKEN, server.send, 'https://venti.example');

		const statistics = await source.statistics();

		expect(statistics?.overall.rows.length).toBeGreaterThan(0);
		expect(server.asked).toEqual([{ line: 'GET /v1/account/statistics', token: TOKEN }]);
	});

	it('hear the server’s refusal in the core’s words', async () => {
		const refusal = new TextEncoder().encode(
			JSON.stringify({ error: { code: 'flight_not_found', message: 'No such flight.' } })
		);
		const id = '5t1eshypdtvrq7rxw39a18vsrg';
		const server = played({ [`DELETE /v1/flights/${id}`]: () => ok(refusal, 404) });
		const source = new OwnFlightsSource(async () => TOKEN, server.send, 'https://venti.example');

		await expect(source.delete(id)).rejects.toMatchObject({ name: 'Server' });
		expect(server.asked).toEqual([{ line: `DELETE /v1/flights/${id}`, token: TOKEN }]);
	});
});
