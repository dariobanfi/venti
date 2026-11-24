import { readFileSync } from 'node:fs';
import { IDBFactory } from 'fake-indexeddb';
import { afterEach, beforeAll, describe, expect, it } from 'vitest';
import { AccountSource, type Send, type ServerAnswer } from './account-source';
import { GroupsSource } from './groups-source';
import { IdbStore } from './idb-store';
import type { PlanDraft, ServerRequest } from './pkg/venti_core_wasm.js';
import { UNREACHABLE } from './protocol';
import { CoreSession } from './session';
import { startCore } from './testing';

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

/** The development mode's group that the demo pilot keeps, and its plan for the 18th. */
const GROUP = '44444444444444444444444444';
const PLAN = '66666666666666666666666666';

function answer(status: number, body: Uint8Array = new Uint8Array()): ServerAnswer {
	return { status, body, retryAfterS: undefined };
}

function refusal(status: number, code: string): ServerAnswer {
	return answer(status, json({ error: { code, message: 'for developers' } }));
}

/**
 * A Venti server played from the recorded answers about groups. It notes
 * what it was asked and with which token. `instead` answers a request
 * differently, or throws to be out of reach.
 */
class Server {
	asked: { method: string; path: string; token: string | undefined; body: string }[] = [];
	instead: (request: ServerRequest) => ServerAnswer | undefined = () => undefined;

	send: Send = async (request, token) => {
		this.asked.push({
			method: request.method,
			path: request.path,
			// Only a request that needs the token may be given it.
			token: request.needsToken ? token : undefined,
			body: text(request.body)
		});
		const other = this.instead(request);
		if (other) return other;
		const { method, path } = request;
		if (method === 'POST' && path === '/v1/sessions') return answer(201, recorded('sign_in.json'));
		if (method === 'GET' && path === '/v1/groups') return answer(200, recorded('groups.json'));
		if (method === 'GET' && path.startsWith('/v1/groups/find')) {
			return answer(200, recorded('groups_found.json'));
		}
		if (method === 'GET' && path.includes('/messages')) {
			return answer(200, recorded('group_messages.json'));
		}
		if (method === 'GET' && path.startsWith('/v1/groups/plans?')) {
			return answer(200, recorded('plan_board.json'));
		}
		if (method === 'POST' && path === '/v1/groups') {
			return answer(201, recorded('group_created.json'));
		}
		if (method === 'POST' && path === '/v1/groups/join') {
			return answer(201, recorded('group_joined.json'));
		}
		if (method === 'POST' && path.endsWith('/invite')) {
			return answer(200, recorded('group_invite.json'));
		}
		if (path.includes('/plans')) {
			return method === 'DELETE' && !path.endsWith('/in')
				? answer(204)
				: answer(method === 'POST' ? 201 : 200, recorded('plan.json'));
		}
		if (method === 'DELETE') return answer(204);
		if (path.startsWith(`/v1/groups/${GROUP}`)) return answer(200, recorded('group.json'));
		return refusal(404, 'not_found');
	};

	/** The requests about groups, without the one that signed in. */
	get about(): Server['asked'] {
		return this.asked.filter((request) => request.path.startsWith('/v1/groups'));
	}
}

const sessions: CoreSession[] = [];

afterEach(() => {
	for (const session of sessions.splice(0)) session.close();
});

/** A browser's store, the core over it, and the way to the server. */
async function browser(server = new Server(), factory = new IDBFactory()) {
	const session = await CoreSession.open(await IdbStore.open('venti', factory));
	sessions.push(session);
	const account = new AccountSource(session, server.send, 'https://venti.example', 'Test browser');
	return { server, factory, session, account, groups: new GroupsSource(session, server.send) };
}

/** The same, signed in as the development mode's demo pilot. */
async function signedIn() {
	const made = await browser();
	await made.account.signIn('demo', 'demo-pilot-password');
	return made;
}

const DRAFT: PlanDraft = {
	day: '2026-11-21',
	place: 'Beispielwiese, Parkplatz',
	position: undefined,
	meetAt: '09:30',
	note: 'A made-up plan.'
};

describe('the board of the pilot’s groups', () => {
	it('is asked for from the pilot’s own day, with the token, and read with the pilot as “you”', async () => {
		const { server, groups } = await signedIn();

		const board = await groups.board('2026-11-18');

		expect(server.about).toEqual([
			{ method: 'GET', path: '/v1/groups/plans?from=2026-11-18', token: TOKEN, body: '' }
		]);
		expect(board.groups).toMatchObject([
			{ id: GROUP, name: 'Mustergruppe', isKeptByMe: true, summary: '3 members · kept by you' }
		]);
		expect(board.days.map((day) => day.day)).toEqual(['2026-11-18']);
		expect(board.days[0].plans[0]).toMatchObject({
			id: PLAN,
			groupId: GROUP,
			groupName: 'Mustergruppe',
			place: 'Musterberg, Startplatz Ost',
			meetAt: '10:30',
			isMine: true,
			canDelete: true,
			amIn: true,
			pilotsIn: ['You', 'Mara Muster', 'Jonas Beispiel'],
			inWords: 'You, Mara Muster and Jonas Beispiel'
		});
		expect(board.isCutShort).toBe(false);
	});

	it('knows who “you” are after a reload, when the account is still in the store and not in memory', async () => {
		const { server, factory } = await signedIn();

		// The site is opened again: a new core over the same store.
		const again = await browser(server, factory);
		const board = await again.groups.board('2026-11-18');

		expect(board.days[0].plans[0]).toMatchObject({ isMine: true, amIn: true });
		expect(board.groups[0].isKeptByMe).toBe(true);
		expect(server.about.at(-1)?.token).toBe(TOKEN);
	});

	it('is not asked for while signed out: the core says so, and nothing is sent', async () => {
		const { server, groups } = await browser();

		await expect(groups.board('2026-11-18')).rejects.toMatchObject({
			name: 'Server',
			message: 'This device is not signed in.'
		});
		await expect(
			groups.createCommunity({ name: 'Hausberg', description: '', home: undefined }, false)
		).rejects.toThrow('This device is not signed in.');
		expect(server.asked).toEqual([]);
	});

	it('says in words that the session has ended, and that the server is out of reach', async () => {
		const { server, groups } = await signedIn();

		server.instead = () => refusal(401, 'unauthorized');
		await expect(groups.board('2026-11-18')).rejects.toThrow(
			'This device is no longer signed in. Sign in again.'
		);

		server.instead = () => {
			const error = new Error('Venti could not be reached.');
			error.name = UNREACHABLE;
			throw error;
		};
		await expect(groups.board('2026-11-18')).rejects.toMatchObject({ name: UNREACHABLE });
	});
});

describe('a group', () => {
	it('is made with the pilot as its owner: private, with its description and home', async () => {
		const { server, groups } = await signedIn();

		const made = await groups.createCommunity(
			{
				name: '  Zweite   Mustergruppe ',
				description: 'Who goes up after work.',
				home: { center: { lat: 47.6642, lon: 11.7967 }, radiusKm: 20, place: 'Musterberg' }
			},
			false
		);

		expect(server.about).toHaveLength(1);
		expect(server.about[0]).toMatchObject({ method: 'POST', path: '/v1/groups', token: TOKEN });
		// The name cleaned as the server cleans it.
		expect(JSON.parse(server.about[0].body)).toMatchObject({
			name: 'Zweite Mustergruppe',
			public: false,
			description: 'Who goes up after work.',
			home: { center: { lat: 47.6642, lon: 11.7967 }, radius_km: 20, place: 'Musterberg' }
		});
		expect(made.group).toMatchObject({ name: 'Zweite Mustergruppe', isKeptByMe: true });
	});

	it('is read with its members, the owner and the pilot marked, and what the pilot may do', async () => {
		const { server, groups } = await signedIn();

		const detail = await groups.community(GROUP);

		expect(server.about).toMatchObject([{ method: 'GET', path: `/v1/groups/${GROUP}` }]);
		expect(detail.members.map((member) => [member.handle, member.role, member.isMe])).toEqual([
			['demo', 'owner', true],
			['mara', 'member', false],
			['jonas', 'member', false]
		]);
		expect(detail).toMatchObject({ canRead: true, canPost: true, canModerate: true });
	});

	it('is joined, an invite sent by handle, handed on, cleared of a member, left and deleted by these requests', async () => {
		const { server, groups } = await signedIn();
		server.instead = (request) =>
			request.method === 'POST' && request.path.endsWith('/members')
				? answer(201, recorded('group_joined.json'))
				: request.method === 'POST' && request.path.endsWith('/invites')
					? answer(
							201,
							json({
								invite: {
									group: JSON.parse(text(recorded('group.json'))).group,
									invited_by: { handle: 'demo', display_name: 'Demo Pilot' },
									invited_at: '2026-11-18T19:05:00Z'
								}
							})
						)
					: undefined;

		await groups.enter(GROUP);
		await groups.invite(GROUP, '@Mara');
		expect((await groups.handOver(GROUP, 'mara')).members).toHaveLength(3);
		await groups.setAdmin(GROUP, 'jonas', true);
		await groups.remove(GROUP, 'jonas');
		await groups.leave(GROUP);
		await groups.delete(GROUP);

		expect(
			server.about.map((request) => [request.method, request.path, request.body, request.token])
		).toEqual([
			['POST', `/v1/groups/${GROUP}/members`, '', TOKEN],
			['POST', `/v1/groups/${GROUP}/invites`, '{"handle":"mara"}', TOKEN],
			['PUT', `/v1/groups/${GROUP}/keeper`, '{"handle":"mara"}', TOKEN],
			['PUT', `/v1/groups/${GROUP}/admins/jonas`, '', TOKEN],
			['DELETE', `/v1/groups/${GROUP}/members/jonas`, '', TOKEN],
			// Leaving is removing oneself.
			['DELETE', `/v1/groups/${GROUP}/members/demo`, '', TOKEN],
			['DELETE', `/v1/groups/${GROUP}`, '', TOKEN]
		]);
	});

	it('is not asked about what the core refuses: a name that is empty or too long', async () => {
		const { server, groups } = await signedIn();

		await expect(
			groups.createCommunity({ name: '   ', description: '', home: undefined }, true)
		).rejects.toThrow('Give the group a name.');
		await expect(
			groups.changeCommunity(GROUP, { name: 'x'.repeat(61), description: '', home: undefined })
		).rejects.toThrow("A group's name has room for 60 characters.");
		expect(server.about).toEqual([]);
	});

	it('says the server’s refusals in the core’s words', async () => {
		const { server, groups } = await signedIn();
		const refused = async (status: number, code: string, work: () => Promise<unknown>) => {
			server.instead = () => refusal(status, code);
			return work().then(
				() => 'not refused',
				(error: Error) => `${error.name}: ${error.message}`
			);
		};

		expect(await refused(404, 'group_not_found', () => groups.community(GROUP))).toMatch(
			/^Server: /
		);
		expect(await refused(404, 'group_member_not_found', () => groups.remove(GROUP, 'jonas'))).toBe(
			'Server: This pilot is no longer in the group.'
		);
		expect(await refused(404, 'plan_not_found', () => groups.setIn(GROUP, PLAN, true))).toBe(
			'Server: This plan is no longer there.'
		);
	});
});

describe('a plan', () => {
	it('is posted, changed, joined, stepped out of and deleted by these requests', async () => {
		const { server, groups } = await signedIn();
		const plan = `/v1/groups/${GROUP}/plans/${PLAN}`;

		const posted = await groups.postPlan(GROUP, DRAFT);
		await groups.changePlan(GROUP, PLAN, { ...DRAFT, meetAt: undefined, note: '' });
		await groups.setIn(GROUP, PLAN, true);
		await groups.setIn(GROUP, PLAN, false);
		await groups.deletePlan(GROUP, PLAN);

		expect(server.about.map((request) => [request.method, request.path, request.token])).toEqual([
			['POST', `/v1/groups/${GROUP}/plans`, TOKEN],
			['PUT', plan, TOKEN],
			['PUT', `${plan}/in`, TOKEN],
			['DELETE', `${plan}/in`, TOKEN],
			['DELETE', plan, TOKEN]
		]);
		expect(JSON.parse(server.about[0].body)).toEqual({
			day: '2026-11-21',
			place: 'Beispielwiese, Parkplatz',
			meet_at: '09:30',
			note: 'A made-up plan.'
		});
		// Without a time and a note, those are left out: a plan is sent whole.
		expect(JSON.parse(server.about[1].body)).toEqual({
			day: '2026-11-21',
			place: 'Beispielwiese, Parkplatz'
		});
		// The answer is the plan, with the pilot in from the start.
		expect(posted).toMatchObject({
			day: '2026-11-21',
			place: 'Beispielwiese, Parkplatz',
			isMine: true,
			amIn: true,
			inWords: 'Only you so far'
		});
	});

	it('is not sent when the core refuses it, and the refusal says what to change', async () => {
		const { server, groups } = await signedIn();
		const refused = (change: Partial<PlanDraft>) =>
			groups.postPlan(GROUP, { ...DRAFT, ...change }).then(
				() => 'not refused',
				(error: Error) => error.message
			);

		expect(await refused({ place: '  ' })).toBe('Say where: the name of the place.');
		expect(await refused({ place: 'x'.repeat(81) })).toBe('The place has room for 80 characters.');
		expect(await refused({ day: '' })).toBe('Choose the day of the plan.');
		expect(await refused({ meetAt: 'half past nine' })).toBe(
			'Write the time like 09:30, on a 24-hour clock.'
		);
		expect(await refused({ note: 'x'.repeat(501) })).toBe('The note has room for 500 characters.');
		expect(server.about).toEqual([]);
	});
});

describe('groups as communities with a chat', () => {
	it('lists the pilot’s groups with the last message, with the token', async () => {
		const { server, groups } = await signedIn();

		const mine = await groups.communities();

		expect(server.about).toEqual([{ method: 'GET', path: '/v1/groups', token: TOKEN, body: '' }]);
		expect(mine).toMatchObject([
			{ id: GROUP, name: 'Mustergruppe', isPublic: false, myRole: 'owner', unread: 0 }
		]);
	});

	it('finds public groups near a place, nearest first, with how far each is', async () => {
		const { server, groups } = await signedIn();

		const found = await groups.find({ lat: 48.137, lon: 11.575 }, '');

		expect(server.about.map((asked) => asked.path)).toEqual([
			'/v1/groups/find?lat=48.1370&lon=11.5750'
		]);
		expect(found[0]).toMatchObject({
			group: { name: 'Munich International Paragliders', isPublic: true, place: 'Munich' },
			// Half a kilometre from the middle of its home.
			distance: 'Here',
			isMember: false
		});
	});

	it('reads a page of messages with the flight’s card and its track', async () => {
		const { groups } = await signedIn();

		const page = await groups.messages('0000000000000000000000g002');

		const withCard = page.messages.find((message) => message.cards.length > 0);
		expect(withCard?.cards[0]).toMatchObject({ kind: 'flight', title: 'Flight by Mara Muster' });
		expect(withCard?.cards[0].track.length).toBeGreaterThan(2);
		expect(page.since).not.toBe('');
	});
});
