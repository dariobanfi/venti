import { readFileSync } from 'node:fs';
import { IDBFactory } from 'fake-indexeddb';
import { afterEach, beforeAll, describe, expect, it } from 'vitest';
import { AccountSource, type Send, type ServerAnswer } from './account-source';
import { IdbStore } from './idb-store';
import type { ServerRequest } from './pkg/venti_core_wasm.js';
import { CoreSession } from './session';
import { SiteEditSource } from './site-edit-source';
import { SitesSource } from './sites-source';
import { startCore } from './testing';
import { type Answer, type Get, WindSource } from './wind-source';

beforeAll(startCore);

/** The server's own recorded answers, which its tests keep current. */
function recorded(name: string): Uint8Array {
	return readFileSync(
		new URL(`../../../../../crates/server/tests/golden/${name}`, import.meta.url)
	);
}

const text = (bytes: Uint8Array | undefined) => new TextDecoder().decode(bytes);
const json = (value: unknown) => new TextEncoder().encode(JSON.stringify(value));

/** The token the recorded sign-in hands out. */
const TOKEN = 'venti_3x7k9q2m4r8t1v6w0y5a2b3c4d5e6f7g8h9j0kmnpqrstvwxyz01';

/** The development mode's made-up take-off that pilots made, and the one with three revisions. */
const KOGEL = 'venti:0000000000000000000000s001';
const JOCH = 'venti:0000000000000000000000s002';

function answer(status: number, body: Uint8Array = new Uint8Array()): ServerAnswer {
	return { status, body, retryAfterS: undefined };
}

/**
 * A Venti server played from the recorded answers about sites that pilots
 * make. It notes what it was sent, with which token, and what was read
 * from it. `instead` answers a write differently.
 */
class Server {
	sent: { method: string; path: string; token: string | undefined; body: string }[] = [];
	read: string[] = [];
	instead: (request: ServerRequest) => ServerAnswer | undefined = () => undefined;

	send: Send = async (request, token) => {
		this.sent.push({
			method: request.method,
			path: request.path,
			// Only a request that needs the token may be given it.
			token: request.needsToken ? token : undefined,
			body: text(request.body)
		});
		const other = this.instead(request);
		if (other) return other;
		if (request.path === '/v1/sessions') return answer(201, recorded('sign_in.json'));
		if (request.method === 'DELETE') return answer(204);
		return answer(request.method === 'POST' ? 201 : 200, recorded('site_written.json'));
	};

	get: Get = async (path): Promise<Answer> => {
		this.read.push(path);
		if (path.endsWith('/history')) return { status: 200, body: recorded('site_history.json') };
		if (path.includes('osm-free-flying')) {
			return { status: 200, body: recorded('site_of_a_source.json') };
		}
		return { status: 200, body: recorded('site.json') };
	};

	/** What was sent about sites, without the request that signed in. */
	get about(): Server['sent'] {
		return this.sent.filter((request) => request.path.startsWith('/v1/sites'));
	}
}

const sessions: CoreSession[] = [];

afterEach(() => {
	for (const session of sessions.splice(0)) session.close();
});

/** A browser signed in as the development mode's demo pilot, and the way to the server. */
async function signedIn() {
	const server = new Server();
	const session = await CoreSession.open(await IdbStore.open('venti', new IDBFactory()));
	sessions.push(session);
	const account = new AccountSource(session, server.send, 'https://venti.example', 'Test browser');
	await account.signIn('demo', 'demo-pilot-password');
	const written = { times: 0 };
	const edits = new SiteEditSource(session, () => (written.times += 1), server.send, server.get);
	return { server, edits, written };
}

/** A take-off a pilot could add: a name, a use, a height and two points of the compass. */
function filled(edits: SiteEditSource) {
	return {
		...edits.draftNew({ lat: 47.27, lon: 10.87 }),
		name: 'Testkogel',
		uses: ['takeoff' as const],
		elevationM: 1500,
		windFromDeg: [225, 247.5]
	};
}

describe('a site a pilot adds or corrects', () => {
	it('has the core’s form: its fields in order, what giving means, and the licence to link', async () => {
		const { edits } = await signedIn();

		const form = edits.form();

		expect(form.fields.map((field) => field.field)).toEqual([
			'name',
			'place',
			'uses',
			'height',
			'wind',
			'description',
			'fee',
			'guests',
			'noFlightTime',
			'website',
			'comment'
		]);
		expect(form.winds).toHaveLength(16);
		expect(form.giving).toContain(form.licence);
		expect(form.licenceUrl).toMatch(/^https:\/\/creativecommons\.org\//);
		expect([form.titleNew, form.sendNew]).toEqual(['Add a site', 'Add for everyone']);
	});

	it('is checked by the core field by field, and not sent while something is wrong', async () => {
		const { server, edits } = await signedIn();
		const empty = edits.draftNew({ lat: 47.27, lon: 10.87 });

		const { faults } = edits.check(empty, '');

		expect(faults.map((fault) => fault.field)).toContain('name');
		expect(faults.find((fault) => fault.field === 'name')?.words).toBe('Give the site a name.');
		await expect(edits.add(empty, '')).rejects.toThrow('Give the site a name.');
		expect(server.about).toEqual([]);

		// The wind as it will be sent, for the line under the points.
		expect(edits.check(filled(edits), '').windInWords).toBe('SW to WSW');
		expect(edits.check(filled(edits), '').faults).toEqual([]);
	});

	it('is added from the pilot’s account, and the answer is the site with the core’s line', async () => {
		const { server, edits, written } = await signedIn();

		const sent = await edits.add(filled(edits), 'Stood there in October.');

		expect(server.about).toHaveLength(1);
		const [request] = server.about;
		expect(request).toMatchObject({ method: 'POST', path: '/v1/sites', token: TOKEN });
		expect(JSON.parse(request.body)).toMatchObject({
			site: { name: 'Testkogel', position: { lat: 47.27, lon: 10.87 }, uses: ['takeoff'] },
			comment: 'Stood there in October.'
		});
		expect(sent.kind).toBe('written');
		if (sent.kind !== 'written') return;
		expect(sent.site.byPilots).toBeDefined();
		expect(sent.words).toBe('Added. The site is on the map for everyone now.');
		// The lists of sites the browser holds are old from now on.
		expect(written.times).toBe(1);
	});

	it('is refused with the site that stands in the way, to open instead', async () => {
		const { server, edits, written } = await signedIn();
		server.instead = (request) =>
			request.path === '/v1/sites' ? answer(409, recorded('site_too_close.json')) : undefined;

		const sent = await edits.add(filled(edits), '');

		expect(sent).toEqual({
			kind: 'refused',
			words:
				'A site that pilots added stands at this place already: Musterkogel West. If it is the same place, correct that one.',
			inTheWay: { id: KOGEL, name: 'Musterkogel West', words: 'Open Musterkogel West' },
			changedMeanwhile: false
		});
		expect(written.times).toBe(0);
	});

	it('is corrected from the draft the core makes of it, and told when someone else was quicker', async () => {
		const { server, edits } = await signedIn();
		const page = await edits.page(KOGEL);
		expect(server.read).toEqual([`/v1/sites/${KOGEL}`]);
		expect(page.canBeCorrected).toBe(true);
		const draft = { ...edits.draftOf(page.site), elevationM: 1871 };
		expect(draft.siteId).toBe(KOGEL);

		const saved = await edits.change(draft, 'Measured again.');

		const [request] = server.about;
		expect(request).toMatchObject({ method: 'PUT', token: TOKEN });
		expect(request.path).toBe(`/v1/sites/${KOGEL}`);
		expect(saved).toMatchObject({
			kind: 'written',
			words: 'Saved for everyone, as revision 3 of this site.'
		});

		// Someone else changed it meanwhile: the core's words, and no site in the way.
		server.instead = () =>
			answer(409, json({ error: { code: 'site_changed', message: 'for developers' } }));
		expect(await edits.change(draft, '')).toEqual({
			kind: 'refused',
			words:
				'Someone else changed this site while you were editing it. Look at how it stands now, then make your change again.',
			inTheWay: undefined,
			changedMeanwhile: true
		});
	});

	it('of another source is not made into a draft', async () => {
		const { edits } = await signedIn();
		const page = await edits.page('osm-free-flying:way/1080986173');

		expect(page.canBeCorrected).toBe(false);
		expect(() => edits.draftOf(page.site)).toThrow();
	});

	it('is removed and put back with the line about it, each answered in the core’s words', async () => {
		const { server, edits, written } = await signedIn();

		expect(await edits.remove(KOGEL, 'Added twice.')).toBe(
			'Removed from the map. Its history stays, and any pilot who may correct the site can bring it back.'
		);
		const back = await edits.restore(KOGEL, 1, 'It is a take-off after all.');

		expect(server.about.map((request) => [request.method, request.token])).toEqual([
			['DELETE', TOKEN],
			['POST', TOKEN]
		]);
		expect(JSON.parse(server.about[1].body)).toEqual({
			restore: 1,
			comment: 'It is a take-off after all.'
		});
		expect(back.site.id).toBe(KOGEL);
		expect(written.times).toBe(2);

		// A refusal is thrown in the core's words.
		server.instead = () =>
			answer(410, json({ error: { code: 'site_gone', message: 'for developers' } }));
		await expect(edits.remove(KOGEL, '')).rejects.toThrow(/^This site was removed\./);
	});

	it('has a history in the core’s lines, with what each revision changed and what undoes the last', async () => {
		const { server, edits } = await signedIn();

		const history = await edits.history(JOCH);

		expect(server.read).toEqual([`/v1/sites/${JOCH}/history`]);
		expect(history.revisions.map((revision) => revision.number)).toEqual([1, 2, 3]);
		expect(history.revisions[1].words).toMatch(/^Changed by Mara Muster \(@mara\) on /);
		expect(history.revisions[1].changes).toEqual(['Height: 2104 m → 1204 m']);
		expect(history.revisions[2].isCurrent).toBe(true);
		expect(history.undoRevision).toBe(2);
		expect(history.undoWords).toBe('Undo the last change');
	});
});

describe('whether the server takes sites from pilots', () => {
	const health = (change: (health: Record<string, unknown>) => void): Get => {
		const said = JSON.parse(text(recorded('health.json')));
		change(said);
		return async () => ({ status: 200, body: json(said) });
	};

	it('is the server’s own word, and no where it does not say', async () => {
		expect(await new WindSource(health(() => {})).takesSitesFromPilots()).toBe(true);
		expect(
			await new WindSource(health((said) => delete said.sites_from_pilots)).takesSitesFromPilots()
		).toBe(false);
		const unreachable: Get = async () => {
			throw new Error('out of reach');
		};
		expect(await new WindSource(unreachable).takesSitesFromPilots()).toBe(false);
	});
});

describe('the list of sites after the pilot wrote one', () => {
	it('is asked for past the browser’s cache, which holds it as it was', async () => {
		const asked: string[] = [];
		const get =
			(how: string): Get =>
			async () => {
				asked.push(how);
				return { status: 200, body: recorded('sites.json') };
			};
		const sites = new SitesSource(async () => true, get('cache'), get('fresh'));
		const box = { south: 47.6, west: 11.7, north: 47.75, east: 11.95 };

		await sites.sites(box);
		sites.wrote();
		await sites.sites(box);

		expect(asked).toEqual(['cache', 'fresh']);
	});
});
