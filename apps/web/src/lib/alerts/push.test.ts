import { describe, expect, it } from 'vitest';
import {
	type HeldSubscription,
	keyBytes,
	type PushBrowser,
	pushOff,
	pushOn,
	type PushServer,
	pushState
} from './push';

/** The key of the server's recorded answer (`crates/server/tests/golden/push_key.json` has its like). */
const KEY =
	'BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8';

/** A browser with the Push API, played: it notes what it was asked. */
function browser(permission: NotificationPermission, answer: NotificationPermission = permission) {
	const did: string[] = [];
	let held: HeldSubscription | null = null;
	let registered = false;
	let key: Uint8Array | undefined;
	const pushManager = {
		getSubscription: async () => held,
		subscribe: async (options: { userVisibleOnly: boolean; applicationServerKey: Uint8Array }) => {
			did.push(`subscribe visible=${options.userVisibleOnly}`);
			key = options.applicationServerKey;
			held = {
				endpoint: 'https://fcm.googleapis.com/fcm/send/abc',
				toJSON: () => ({
					endpoint: 'https://fcm.googleapis.com/fcm/send/abc',
					keys: { p256dh: 'BPhp', auth: 'Bwc' }
				}),
				unsubscribe: async () => {
					did.push('unsubscribe');
					held = null;
					return true;
				}
			};
			return held;
		}
	};
	const played: PushBrowser = {
		permission: () => permission,
		ask: async () => {
			did.push('ask');
			permission = answer;
			return answer;
		},
		registered: async () => (registered ? { pushManager } : undefined),
		register: async () => {
			did.push('register');
			registered = true;
			return { pushManager };
		}
	};
	return { played, did, key: () => key };
}

function server(refuse?: Error) {
	const told: string[] = [];
	const played: PushServer = {
		pushKey: async () => KEY,
		pushSubscribe: async (subscribed) => {
			if (refuse) throw refuse;
			told.push(`subscribe ${subscribed.endpoint} ${subscribed.p256dh} ${subscribed.auth}`);
		},
		pushUnsubscribe: async (endpoint) => {
			told.push(`unsubscribe ${endpoint}`);
		}
	};
	return { played, told };
}

describe('notify this browser', () => {
	it('reads the server’s key into the 65 bytes of an uncompressed P-256 point', () => {
		const bytes = keyBytes(KEY);
		expect(bytes.length).toBe(65);
		expect(bytes[0]).toBe(4);
	});

	it('says a browser without the Push API cannot be told', async () => {
		expect(await pushState(undefined)).toBe('unsupported');
	});

	it('asks, registers the worker, subscribes with the key and tells the server', async () => {
		const { played, did, key } = browser('default', 'granted');
		const venti = server();
		expect(await pushState(played)).toBe('off');

		expect(await pushOn(played, venti.played)).toBe('on');

		expect(did).toEqual(['ask', 'register', 'subscribe visible=true']);
		expect(key()).toEqual(keyBytes(KEY));
		expect(venti.told).toEqual(['subscribe https://fcm.googleapis.com/fcm/send/abc BPhp Bwc']);
		expect(await pushState(played)).toBe('on');
	});

	it('asks nothing more of a pilot who said no', async () => {
		const { played, did } = browser('default', 'denied');
		const venti = server();
		expect(await pushOn(played, venti.played)).toBe('denied');
		expect(did).toEqual(['ask']);
		expect(venti.told).toEqual([]);
		expect(await pushState(played)).toBe('denied');
	});

	it('ends the browser’s subscription again when the server refuses it', async () => {
		const { played, did } = browser('granted');
		const refusal = new Error('This account has as many browsers as it may.');
		await expect(pushOn(played, server(refusal).played)).rejects.toBe(refusal);
		expect(did).toEqual(['register', 'subscribe visible=true', 'unsubscribe']);
		expect(await pushState(played)).toBe('off');
	});

	it('switched off, unsubscribes and tells the server the endpoint', async () => {
		const { played, did } = browser('granted');
		const venti = server();
		await pushOn(played, venti.played);

		expect(await pushOff(played, venti.played)).toBe('off');

		expect(did.at(-1)).toBe('unsubscribe');
		expect(venti.told.at(-1)).toBe('unsubscribe https://fcm.googleapis.com/fcm/send/abc');
	});
});
