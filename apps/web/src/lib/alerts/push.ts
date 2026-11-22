/**
 * "Notify this browser": the plumbing of Web Push for the alarms. Switched
 * on, the browser is asked for its permission, the app's small service
 * worker (`static/push-worker.js`) is registered, the browser subscribes at
 * its vendor's push service with the server's key, and the server is told
 * the subscription; switched off, the subscription is ended and the server
 * told. The server then sends what fires to the browser also while no
 * Venti page is open (`docs/api.md`, "Web Push").
 *
 * The requests and every refusal are the core's, through the worker
 * (`core/social-source.ts`). What the browser offers is handed in, so that
 * the steps are tested without one.
 */

import type { PushSubscribed } from '#lib/core/index.ts';

/** How the switch stands in this browser. */
export type PushState =
	/** The browser has no Push API: the row says so and is disabled. */
	| 'unsupported'
	/** The pilot told the browser never to show Venti's notifications. */
	| 'denied'
	| 'off'
	| 'on';

/** What the core's worker does for the switch. */
export interface PushServer {
	pushKey(): Promise<string>;
	pushSubscribe(subscribed: PushSubscribed): Promise<unknown>;
	pushUnsubscribe(endpoint: string): Promise<unknown>;
}

/** A subscription as the browser holds it. */
export interface HeldSubscription {
	endpoint: string;
	toJSON(): { endpoint?: string; keys?: Record<string, string> };
	unsubscribe(): Promise<boolean>;
}

/** The browser's push manager, as far as it is used. */
export interface PushManagerLike {
	getSubscription(): Promise<HeldSubscription | null>;
	subscribe(options: {
		userVisibleOnly: boolean;
		applicationServerKey: Uint8Array<ArrayBuffer>;
	}): Promise<HeldSubscription>;
}

/** What a browser offers for Web Push. Nothing of it where it has no Push API. */
export interface PushBrowser {
	/** "granted", "denied" or "default". */
	permission(): NotificationPermission;
	/** Asks the pilot; the answer. */
	ask(): Promise<NotificationPermission>;
	/** The registration of the app's push worker, if it was registered before. */
	registered(): Promise<{ pushManager: PushManagerLike } | undefined>;
	/** Registers the app's push worker and waits until it is active. */
	register(): Promise<{ pushManager: PushManagerLike }>;
}

/** The server's key as `pushManager.subscribe` takes it: base64url, without padding, to bytes. */
export function keyBytes(base64url: string): Uint8Array<ArrayBuffer> {
	const base64 = base64url.replaceAll('-', '+').replaceAll('_', '/');
	const padded = base64 + '='.repeat((4 - (base64.length % 4)) % 4);
	const binary = atob(padded);
	const bytes = new Uint8Array(new ArrayBuffer(binary.length));
	for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index);
	return bytes;
}

/** A subscription as the server takes it. Throws when the browser left a part out. */
function subscribedOf(held: HeldSubscription): PushSubscribed {
	const json = held.toJSON();
	const endpoint = json.endpoint ?? held.endpoint;
	const p256dh = json.keys?.p256dh;
	const auth = json.keys?.auth;
	if (!endpoint || !p256dh || !auth) {
		throw new Error('This browser gave a subscription without its keys.');
	}
	return { endpoint, p256dh, auth };
}

/** How the switch stands: on while the browser holds a subscription and may show notifications. */
export async function pushState(browser: PushBrowser | undefined): Promise<PushState> {
	if (!browser) return 'unsupported';
	const permission = browser.permission();
	if (permission === 'denied') return 'denied';
	if (permission !== 'granted') return 'off';
	const registration = await browser.registered();
	const held = await registration?.pushManager.getSubscription();
	return held ? 'on' : 'off';
}

/**
 * Switches on: permission, the worker, the subscription, the server. The
 * state afterwards: "denied" when the pilot said no. Throws, in the core's
 * words, when the server refuses; the browser's subscription is ended
 * again then, so that the switch does not say more than is true.
 */
export async function pushOn(browser: PushBrowser, server: PushServer): Promise<PushState> {
	const permission =
		browser.permission() === 'default' ? await browser.ask() : browser.permission();
	if (permission === 'denied') return 'denied';
	if (permission !== 'granted') return 'off';
	const key = keyBytes(await server.pushKey());
	const registration = await browser.register();
	const held =
		(await registration.pushManager.getSubscription()) ??
		(await registration.pushManager.subscribe({
			userVisibleOnly: true,
			applicationServerKey: key
		}));
	try {
		await server.pushSubscribe(subscribedOf(held));
	} catch (error) {
		await held.unsubscribe().catch(() => false);
		throw error;
	}
	return 'on';
}

/**
 * Switches off: the browser's subscription is ended and the server told.
 * The browser's side counts: when the server cannot be told, it finds the
 * subscription ended the next time it sends and removes it then.
 */
export async function pushOff(browser: PushBrowser, server: PushServer): Promise<PushState> {
	const registration = await browser.registered();
	const held = await registration?.pushManager.getSubscription();
	if (held) {
		const { endpoint } = held;
		await held.unsubscribe();
		await server.pushUnsubscribe(endpoint).catch(() => {});
	}
	return pushState(browser);
}

/**
 * This browser, where it has the Push API, service workers and
 * notifications; nothing where it lacks one of them. `workerUrl` is the
 * app's push worker, a file of the app itself.
 */
export function thisBrowser(workerUrl: string, scope: string): PushBrowser | undefined {
	if (
		typeof navigator === 'undefined' ||
		!('serviceWorker' in navigator) ||
		typeof PushManager === 'undefined' ||
		typeof Notification === 'undefined'
	) {
		return undefined;
	}
	return {
		permission: () => Notification.permission,
		ask: () => Notification.requestPermission(),
		registered: async () => (await navigator.serviceWorker.getRegistration(scope)) ?? undefined,
		register: async () => {
			await navigator.serviceWorker.register(workerUrl, { scope });
			// Subscribing needs a worker that is active.
			return navigator.serviceWorker.ready;
		}
	};
}
