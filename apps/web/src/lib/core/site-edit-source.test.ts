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
		if (request.path === '/v1/account') return answer(200, recorded('account.json'));
		if (request.path.endsWith('/confirmation')) {
			return request.method === 'DELETE' ? answer(204) : answer(200, recorded('site.json'));
		}
		if (request.method === 'DELETE') return answer(204);
		return answer(request.method === 'POST' ? 201 : 200, recorded('site_written.json'));
	};

	get: Get = async (path): Promise<Answer> => {
		this.read.push(path);
		if (path.endsWith('/history')) return { status: 200, body: recorded('site_history.json') };
		if (path.endsWith('/confirmations')) {
			return { status: 200, body: recorded('site_confirmations.json') };
		}
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
	return { server, edits, written, account };
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

/** The server's word about an account that is no partner of Venti for flying sites. */
function ordinary(): ServerAnswer {
	const said = JSON.parse(text(recorded('account.json')));
	said.account.edits_sites = false;
	return answer(200, json(said));
}

describe('what may be offered for a site, and to whom', () => {
	it('is the core’s answer for a partner: the partners’ entry, and correcting a partner’s site', async () => {
		const { edits } = await signedIn();
		const { site } = await edits.page(KOGEL);

		// The recorded sign-in is the development mode's one partner.
		expect(await edits.adding(true)).toMatchObject({
			mayAdd: true,
			asPartner: true,
			layer: 'venti',
			entryWords: 'Add a site',
			title: 'Add a site',
			sendWords: 'Add for everyone'
		});
		// A partner adds whatever the server says of pilots.
		expect((await edits.adding(false)).mayAdd).toBe(true);
		const offers = await edits.offers(site, true, undefined);
		expect(offers.correct).toEqual({ isOffered: true, words: 'Correct this site' });
		expect(offers.remove).toEqual({ isOffered: true, words: 'Remove this site' });
		expect(offers.confirm).toEqual({ isOffered: true, words: 'I took off here and it was good' });
		expect(offers.note).toBe('');
	});

	it('follows what the server says of the account now: an ordinary pilot adds a place from pilots', async () => {
		const { server, edits, account } = await signedIn();
		const { site } = await edits.page(KOGEL);
		server.instead = (request) => (request.path === '/v1/account' ? ordinary() : undefined);

		const view = await account.refresh();

		// Asked with the session, and remembered by the core.
		expect(server.sent.at(-1)).toMatchObject({ method: 'GET', path: '/v1/account', token: TOKEN });
		expect(view?.mayEditPartnerSites ?? false).toBe(false);
		expect((await account.account())?.mayEditPartnerSites ?? false).toBe(false);
		const adding = await edits.adding(true);
		expect(adding).toMatchObject({
			mayAdd: true,
			asPartner: false,
			layer: 'venti-pilots',
			entryWords: 'Add a place pilots take off from',
			title: 'A place pilots take off from',
			sendWords: 'Add as a place from pilots'
		});
		expect(adding.giving).toContain('pilots who switch on the sites from pilots');
		// Where the server takes none, nothing is added, and the core says why.
		expect(await edits.adding(false)).toMatchObject({
			mayAdd: false,
			entryWords: '',
			whyNot: 'This server takes no places from pilots yet.'
		});
		// A partner's site is not theirs to change: reported, confirmed, and told why.
		const offers = await edits.offers(site, true, undefined);
		expect(offers.correct).toEqual({ isOffered: false, words: '' });
		expect(offers.remove.isOffered).toBe(false);
		expect(offers.report).toEqual({ isOffered: true, words: 'Report this site' });
		expect(offers.confirm.isOffered).toBe(true);
		expect(offers.note).toBe(
			"A partner of Venti added this site, and only Venti's partners change it. If something about it is wrong, report it."
		);
	});

	it('keeps what it knew when the server gives no answer, and signs nobody out', async () => {
		const { server, edits, account } = await signedIn();
		server.instead = (request) => (request.path === '/v1/account' ? answer(503) : undefined);

		const view = await account.refresh();

		expect(view?.needsSignIn).toBe(false);
		expect((await edits.adding(true)).asPartner).toBe(true);
	});

	it('says to sign in again once the server no longer knows the session', async () => {
		const { server, edits, account } = await signedIn();
		const { site } = await edits.page(KOGEL);
		server.instead = (request) => (request.path === '/v1/account' ? answer(401) : undefined);

		const view = await account.refresh();

		expect(view?.needsSignIn).toBe(true);
		expect(await edits.adding(true)).toMatchObject({
			mayAdd: false,
			whyNot: 'Sign in again to add a place pilots take off from.'
		});
		const offers = await edits.offers(site, true, undefined);
		expect(offers.correct.isOffered).toBe(false);
		expect(offers.history.isOffered).toBe(true);
		expect(offers.note).toBe(edits.words().signedInAgain);
	});

	it('offers nothing for a site of a source, which is corrected at its source', async () => {
		const { edits } = await signedIn();
		const { site } = await edits.page('osm-free-flying:way/1080986173');

		const offers = await edits.offers(site, true, undefined);

		expect(
			[offers.correct, offers.remove, offers.confirm, offers.report, offers.history].map(
				(offer) => offer.isOffered
			)
		).toEqual([false, false, false, false, false]);
		expect(offers.note).toMatch(/^This site is its source's record/);
	});
});

describe('“I took off here and it was good”', () => {
	it('has the core’s form: the days from today back, and the line with its limit', async () => {
		const { edits } = await signedIn();

		const form = edits.confirmForm('2026-11-18');

		expect(form.days.slice(0, 3)).toEqual([
			{ day: '2026-11-18', words: 'Today' },
			{ day: '2026-11-17', words: 'Yesterday' },
			{ day: '2026-11-16', words: 'Mon 16 Nov' }
		]);
		expect(form.noteAtMost).toBe(200);
		expect(form.send).toBe('Confirm');
	});

	it('is sent from the pilot’s account for a day, and answered with the core’s sentence', async () => {
		const { server, edits, written } = await signedIn();

		const confirmed = await edits.confirm(KOGEL, '2026-11-17', '2026-11-18', 'Smooth.', undefined);

		const [request] = server.about;
		expect(request).toMatchObject({
			method: 'PUT',
			path: `/v1/sites/${KOGEL}/confirmation`,
			token: TOKEN
		});
		expect(JSON.parse(request.body)).toEqual({ day: '2026-11-17', note: 'Smooth.' });
		expect(confirmed.site.id).toBe(KOGEL);
		expect(confirmed.words).toBe('You confirmed this site for Tue 17 Nov.');
		expect(written.times).toBe(1);
		// The logbook is empty: no flight of that day began here.
		expect(await edits.confirmFlights(confirmed.site, '2026-11-17', 3600)).toEqual([]);

		// A day that is to come is refused in the core's words before anything is sent.
		await expect(edits.confirm(KOGEL, '2026-11-19', '2026-11-18', '', undefined)).rejects.toThrow();
		expect(server.about).toHaveLength(1);
	});

	it('is listed in the core’s lines, and taken back', async () => {
		const { server, edits } = await signedIn();
		const { site } = await edits.page(KOGEL);

		const list = await edits.confirmations(KOGEL);

		expect(server.read.at(-1)).toBe(`/v1/sites/${KOGEL}/confirmations`);
		expect(list.pilots).toBe(2);
		expect(list.words).toBe('2 pilots took off here, last on 16 November 2026');
		expect(list.lines[0].words).toBe(
			'Mara Muster (@mara) on Mon 16 Nov 2026: “North-west at noon, smooth.”'
		);
		// The demo pilot is not among them: nothing to take back is offered.
		const offers = await edits.offers(site, true, list);
		expect(offers.hasConfirmed).toBe(false);
		expect(offers.withdraw.isOffered).toBe(false);

		const words = await edits.withdraw(KOGEL);

		expect(server.about.at(-1)).toMatchObject({
			method: 'DELETE',
			path: `/v1/sites/${KOGEL}/confirmation`,
			token: TOKEN
		});
		expect(words).not.toBe('');
	});
});

describe('a draft when someone else changed the site meanwhile', () => {
	it('is carried onto the site as it stands by the core: the pilot’s entries stay', async () => {
		const { edits } = await signedIn();
		const { site: base } = await edits.page(KOGEL);
		const edited = { ...edits.draftOf(base), elevationM: 1871 };
		// Meanwhile someone renamed the site, as its third revision.
		const now = {
			...base,
			name: 'Musterkogel Nord',
			byPilots: base.byPilots && { ...base.byPilots, revision: base.byPilots.revision + 1 }
		};

		const rebased = edits.rebased(base, edited, now);

		expect(rebased.draft).toMatchObject({
			siteId: KOGEL,
			revision: 3,
			name: 'Musterkogel Nord',
			elevationM: 1871
		});
		expect(rebased.kept).toEqual(['height']);
		expect(rebased.clashes).toEqual([]);
		expect(rebased.theirs).toEqual(['Name: “Musterkogel West” → “Musterkogel Nord”']);
		expect(rebased.words).toMatch(/^Someone else changed this site while you were editing it\./);

		// Both changed the height, and not to the same: the pilot's stays, and is named.
		const clash = edits.rebased(base, edited, { ...now, elevationM: 1850 });
		expect(clash.draft.elevationM).toBe(1871);
		expect(clash.clashes).toEqual(['height']);
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

	it('holds the places from pilots only when it was asked for with them', async () => {
		const asked: string[] = [];
		const get: Get = async (path) => {
			asked.push(path);
			return {
				status: 200,
				body: recorded(path.includes('venti-pilots') ? 'sites_venti_pilots.json' : 'sites.json')
			};
		};
		const sites = new SitesSource(async () => true, get, get);
		const box = { south: 47.2, west: 10.8, north: 47.4, east: 11 };

		const without = await sites.sites(box);
		const withPilots = await sites.sites(box, true);

		expect(asked[0]).not.toContain('venti-pilots');
		expect(asked[1]).toContain('venti-pilots');
		expect(without?.sites.some((site) => site.byPilots?.fromPilots)).toBe(false);
		const joch = withPilots?.sites.find((site) => site.name === 'Beispieljoch');
		expect(joch?.byPilots).toMatchObject({ fromPilots: true, label: 'From pilots' });
		expect(joch?.byPilots?.note).toMatch(/^Added by pilots on Venti: not official/);
	});
});
