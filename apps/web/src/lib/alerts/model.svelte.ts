/**
 * The pilot's alarms as the screens see them: the list, what fired, and
 * when the browser fetches them.
 *
 * What an alarm is, its line, its faults and the lines of what fired are
 * the core's (`crates/core/src/alerts.rs`), and the requests are sent by
 * the worker (`core/alerts-source.ts`). This class keeps what the screens
 * draw, runs the fetch when the account's sync runs (the site opened or
 * back at the front, after signing in) and every five minutes while a page
 * is open, and after an alarm was saved; and it tells the pilot of what
 * fired: the browser's notification where the pilot allowed it. The count
 * of what is new is the server's, on the pilot's picture
 * (`social/unread.svelte.ts`).
 */

import type {
	AlertDraft,
	AlertFaultView,
	AlertFormWords,
	AlertView,
	CoreClient,
	FiredLine,
	SiteView,
	WindStation
} from '#lib/core/index.ts';
import { isInWords } from '#lib/core/index.ts';
import { reasonFor } from '#lib/logbook/model.svelte.ts';

/** How often the alarms are fetched while a page of the site is open, in milliseconds. */
export const ALERTS_EVERY_MS = 5 * 60_000;

/** What the browser can show of a notification; `Notification` where it has one. */
export interface Notifier {
	/** "granted", "denied" or "default", as the browser says. */
	permission(): NotificationPermission | undefined;
	ask(): Promise<void>;
	show(line: FiredLine, onclick: () => void): void;
}

/** The browser's own notifications, where it has them. */
export const browserNotifier: Notifier = {
	permission: () => (typeof Notification === 'undefined' ? undefined : Notification.permission),
	ask: async () => {
		if (typeof Notification === 'undefined' || Notification.permission !== 'default') return;
		try {
			await Notification.requestPermission();
		} catch {
			// A browser that will not ask has the bubble on the pilot's picture.
		}
	},
	show: (line, onclick) => {
		const shown = new Notification(line.title, {
			body: line.line,
			tag: `${line.alertId}:${line.firedAtUnixS}`
		});
		shown.onclick = () => {
			globalThis.focus?.();
			onclick();
			shown.close();
		};
	}
};

/** How far the clock is ahead of UTC at a moment, in seconds. */
export function utcOffsetS(atMs: number): number {
	return -new Date(atMs).getTimezoneOffset() * 60;
}

/** A wind alarm for a station, as its page starts the form: its name filled in. */
export function windDraft(station: Pick<WindStation, 'id' | 'name'>): AlertDraft {
	return {
		...EMPTY,
		name: station.name,
		kind: 'wind',
		stationId: station.id,
		stationName: station.name
	};
}

/** A day alarm for a take-off, as its page starts the form: today and tomorrow, a cross-country day. */
export function dayDraft(site: Pick<SiteView, 'id' | 'name' | 'position'>): AlertDraft {
	return {
		...EMPTY,
		name: site.name,
		kind: 'day',
		lat: site.position.lat,
		lon: site.position.lon,
		siteId: site.id,
		placeName: site.name,
		daysAhead: [0, 1],
		atLeast: 'crossCountry'
	};
}

const EMPTY: AlertDraft = {
	name: '',
	enabled: true,
	kind: 'wind',
	stationId: '',
	stationName: '',
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

export class AlertsModel {
	/** The account's alarms as the browser holds them. */
	alerts = $state.raw<AlertView[]>([]);
	/** What they fired, the newest first. */
	fired = $state.raw<FiredLine[]>([]);
	/** Whether what the browser holds was read once. */
	read = $state(false);
	/** Why the last fetch stopped, in words. */
	problem = $state.raw<string>();
	/** Every word of the alarms' screens, the core's, once `words()` was asked. */
	said = $state.raw<AlertFormWords>();

	#core: CoreClient;
	#notifier: Notifier;
	#now: () => number;
	#open: () => void;
	#points: Promise<string[]> | undefined;
	#words: Promise<AlertFormWords> | undefined;
	#syncing: Promise<void> | undefined;

	constructor(
		core: CoreClient,
		open: () => void,
		notifier: Notifier = browserNotifier,
		now: () => number = Date.now
	) {
		this.#core = core;
		this.#open = open;
		this.#notifier = notifier;
		this.#now = now;
	}

	/** The sixteen points of the compass, the core's, asked for once. */
	points(): Promise<string[]> {
		this.#points ??= this.#core.alertPoints();
		this.#points.catch(() => (this.#points = undefined));
		return this.#points;
	}

	/** Every word of the alarms' screens, the core's, asked for once; `said` holds them then. */
	words(): Promise<AlertFormWords> {
		this.#words ??= this.#core.alertFormWords().then((words) => (this.said = words));
		this.#words.catch(() => (this.#words = undefined));
		return this.#words;
	}

	line(draft: AlertDraft): Promise<string> {
		return this.#core.alertLine($state.snapshot(draft) as AlertDraft);
	}

	check(draft: AlertDraft): Promise<AlertFaultView[]> {
		return this.#core.alertCheck($state.snapshot(draft) as AlertDraft);
	}

	/** Reads what the browser holds: after a fetch, after signing in or out. */
	async reread(): Promise<void> {
		try {
			const now = this.#now();
			const held = await this.#core.alertsHeld(Math.floor(now / 1000), utcOffsetS(now));
			this.alerts = held.alerts;
			this.fired = held.fired;
		} catch {
			// The screens keep what they have.
		}
		this.read = true;
	}

	/**
	 * Fetches the alarms and what fired. Quiet: being offline is no news,
	 * and a session that ended is the account's to say. One at a time.
	 */
	sync(): Promise<void> {
		this.#syncing ??= this.#sync().finally(() => (this.#syncing = undefined));
		return this.#syncing;
	}

	async #sync(): Promise<void> {
		try {
			const synced = await this.#core.alertsSync(utcOffsetS(this.#now()));
			if (synced?.kind === 'failed') this.problem = synced.message;
			if (synced?.kind === 'finished') {
				this.problem = undefined;
				this.#tell(synced.newLines);
			}
		} catch (error) {
			this.problem = words(error);
		}
		await this.reread();
	}

	#tell(lines: FiredLine[]): void {
		if (this.#notifier.permission() !== 'granted') return;
		for (const line of lines) this.#notifier.show(line, this.#open);
	}

	/**
	 * Saves an alarm: a new one, or the one of that identifier. The first
	 * one saved asks the browser to let the site notify. Throws in words.
	 */
	async save(id: string | undefined, draft: AlertDraft): Promise<void> {
		const first = this.alerts.length === 0;
		const plain = $state.snapshot(draft) as AlertDraft;
		try {
			if (id) await this.#core.changeAlert(id, plain);
			else await this.#core.addAlert(plain);
		} catch (error) {
			throw new Error(words(error), { cause: error });
		}
		if (first && !id) void this.#notifier.ask();
		await this.#syncAgain();
	}

	/** Switches an alarm on or off. Throws in words. */
	async switch(alert: AlertView, enabled: boolean): Promise<void> {
		if (!alert.draft) return;
		await this.save(alert.id, { ...alert.draft, enabled });
	}

	/** Removes an alarm. Throws in words. */
	async remove(id: string): Promise<void> {
		try {
			await this.#core.removeAlert(id);
		} catch (error) {
			throw new Error(words(error), { cause: error });
		}
		await this.#syncAgain();
	}

	/** A fetch that starts after a change: one under way may have asked before it. */
	async #syncAgain(): Promise<void> {
		await this.#syncing;
		await this.sync();
	}
}

function words(error: unknown): string {
	if (isInWords(error)) return (error as Error).message;
	return reasonFor(error);
}
