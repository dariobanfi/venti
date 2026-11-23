import { readFileSync } from 'node:fs';
import { afterEach, beforeAll, describe, expect, it } from 'vitest';
import { AccountSource, type Send, type ServerAnswer } from './account-source';
import type { ServerRequest } from './pkg/venti_core_wasm.js';
import { CoreSession } from './session';
import { SocialSource } from './social-source';
import { MemoryStore } from './store';
import { startCore } from './testing';

beforeAll(startCore);

/** The server's own recorded answers, which its tests keep current. */
function recorded(name: string): Uint8Array {
	return readFileSync(
		new URL(`../../../../../crates/server/tests/golden/${name}`, import.meta.url)
	);
}

const text = (bytes: Uint8Array | undefined) => new TextDecoder().decode(bytes);
const FLIGHT = '5t1eshypdtvrq7rxw39a18vsrg';
/** 18 November 2026, 19:05 UTC: the development server's clock. */
const NOW_S = Date.UTC(2026, 10, 18, 19, 5) / 1000;

function answer(status: number, body: Uint8Array = new Uint8Array()): ServerAnswer {
	return { status, body, retryAfterS: undefined };
}

/** A Venti server played from the recorded answers. It notes what it was asked, and with which token. */
class Server {
	asked: { method: string; path: string; token: string | undefined; body: string }[] = [];

	send: Send = async (request: ServerRequest, token) => {
		this.asked.push({
			method: request.method,
			path: request.path,
			token: request.needsToken ? token : undefined,
			body: text(request.body)
		});
		const { method, path } = request;
		if (method === 'POST' && path === '/v1/sessions') return answer(201, recorded('sign_in.json'));
		if (path.endsWith('/followers')) return answer(200, recorded('followers.json'));
		if (path.endsWith('/follow')) return answer(200, recorded('follow.json'));
		if (path.includes('/feed')) return answer(200, recorded('feed.json'));
		if (method === 'GET' && path.includes('/cheers')) return answer(200, recorded('cheers.json'));
		if (path.endsWith('/cheer')) return answer(200, recorded('cheer.json'));
		if (method === 'GET' && path.includes('/comments')) {
			return answer(200, recorded('comments.json'));
		}
		if (method === 'POST' && path.endsWith('/comments')) {
			return answer(201, recorded('comment.json'));
		}
		if (path.includes('/notifications')) return answer(200, recorded('notifications.json'));
		if (path === '/v1/account/unread') return answer(200, recorded('unread.json'));
		if (path === '/v1/push/key') return answer(200, recorded('push_key.json'));
		if (path === '/v1/account/push') return answer(204);
		return answer(
			404,
			new TextEncoder().encode('{"error":{"code":"not_found","message":"for developers"}}')
		);
	};
}

let session: CoreSession;
afterEach(() => session?.close());

async function open(signIn: boolean): Promise<{ server: Server; social: SocialSource }> {
	const server = new Server();
	session = await CoreSession.open(new MemoryStore());
	if (signIn) {
		const account = new AccountSource(session, server.send, 'https://venti.example', 'Test');
		await account.signIn('demo', 'demo-pilot-password');
	}
	server.asked = [];
	return { server, social: new SocialSource(session, async () => NOW_S, server.send) };
}

describe('pilots among each other', () => {
	it('has the core’s words that no answer carries', async () => {
		const { social } = await open(false);
		const words = social.words();
		expect(words.feedEveryone).toBe('Everyone');
		expect(words.feedFollowing).toBe('Following');
		expect(words.notificationsTitle).toBe('Notifications');
	});

	it('reads a pilot’s followers with the counts in words, and sends a token only when signed in', async () => {
		const out = await open(false);
		const list = await out.social.follows('lena');
		expect(list.followersLine).toMatch(/follower/);
		expect(list.followingLine).toMatch(/^Follows /);
		expect(out.server.asked[0].token).toBeUndefined();
		session.close();

		const signed = await open(true);
		await signed.social.follows('lena');
		expect(signed.server.asked[0].token).toBeDefined();
	});

	it('asks nothing for the feed or the notifications of a browser that is not signed in', async () => {
		const { server, social } = await open(false);
		expect(await social.feed()).toBeUndefined();
		expect(await social.notifications()).toBeUndefined();
		expect(await social.unread()).toBeUndefined();
		expect(await social.unreadSeen(undefined, undefined)).toBeUndefined();
		expect(server.asked).toEqual([]);
	});

	it('reads the feed, the cheers, the comments and what the pilot was told', async () => {
		const { social } = await open(true);
		const feed = await social.feed();
		expect(feed?.rows.length).toBeGreaterThan(0);
		expect(feed?.rows[0].flight.id).toBeTruthy();

		const cheers = await social.cheers(FLIGHT);
		expect(cheers.line).toMatch(/cheered/);
		expect(['Cheer', 'Cheered']).toContain(cheers.button);

		const comments = await social.comments(FLIGHT, 'demo');
		expect(comments.countLine).toBe('2 comments');
		expect(comments.comments[0].byline).toBe('Mara Muster · 55 min ago');
		// The flight's pilot may remove what is written under it.
		expect(comments.comments.every((comment) => comment.canRemove)).toBe(true);

		const told = await social.notifications();
		expect(told?.lines.length).toBeGreaterThan(0);
		expect(told?.lines.some((line) => line.flightId === FLIGHT)).toBe(true);
	});

	it('gives a comment’s day, not an age, while the server’s clock is not known', async () => {
		const { server } = await open(true);
		const social = new SocialSource(session, async () => 0, server.send);
		const comments = await social.comments(FLIGHT, 'demo');
		expect(comments.comments[0].byline).toMatch(/^Mara Muster · /);
		expect(comments.comments[0].byline).not.toMatch(/ago/);
		const told = await social.notifications();
		expect(told?.lines.some((line) => /ago/.test(JSON.stringify(line)))).toBe(false);
	});

	it('looks at a comment as it is written, and posts it with the token', async () => {
		const { server, social } = await open(true);
		expect(social.draft('   ').canSend).toBe(false);
		const draft = social.draft(' Well flown ');
		expect(draft.canSend).toBe(true);
		expect(draft.text).toBe('Well flown');

		const posted = await social.comment(FLIGHT, draft.text);
		expect(posted.comment.text).toBeTruthy();
		expect(server.asked[0].method).toBe('POST');
		expect(server.asked[0].token).toBeDefined();
		expect(JSON.parse(server.asked[0].body)).toEqual({ text: 'Well flown' });
	});

	it('fetches the server’s push key, and sends and removes a subscription', async () => {
		const { server, social } = await open(true);
		expect(await social.pushKey()).toMatch(/^B[A-Za-z0-9_-]{86}$/);
		await social.pushSubscribe({
			endpoint: 'https://fcm.googleapis.com/fcm/send/abc',
			p256dh: 'BPhp',
			auth: 'Bwc'
		});
		await social.pushUnsubscribe('https://fcm.googleapis.com/fcm/send/abc');
		const [, subscribed, removed] = server.asked;
		expect([subscribed.method, subscribed.path]).toEqual(['POST', '/v1/account/push']);
		expect(JSON.parse(subscribed.body)).toEqual({
			endpoint: 'https://fcm.googleapis.com/fcm/send/abc',
			keys: { p256dh: 'BPhp', auth: 'Bwc' }
		});
		expect([removed.method, removed.path]).toEqual(['DELETE', '/v1/account/push']);
		expect(JSON.parse(removed.body)).toEqual({
			endpoint: 'https://fcm.googleapis.com/fcm/send/abc'
		});
		expect(social.pushLine()).toMatch(/alarm fires/);
	});

	it('reads what is new for the badge, and tells the server what the pilot saw', async () => {
		const { server, social } = await open(true);
		const found = await social.unread();
		expect(found).toMatchObject({ notifications: 4, alarms: 1, total: 5, badge: '5' });
		expect(found?.label).toBe('5 new');
		expect(server.asked[0]).toMatchObject({ method: 'GET', path: '/v1/account/unread' });
		expect(server.asked[0].token).toBeDefined();

		await social.unreadSeen('9', '0');
		expect(server.asked[1]).toMatchObject({ method: 'PUT', path: '/v1/account/unread' });
		expect(JSON.parse(server.asked[1].body)).toEqual({
			notifications_cursor: '9',
			alarms_cursor: '0'
		});
	});
});
