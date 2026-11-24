/**
 * The marker of what is new for the pilot: the bubble on their picture in
 * the top bar. The count is the server's, the same on every device: the
 * notifications and the lines of what the alarms fired after the
 * account's mark of what the pilot saw. The badge and the words a screen
 * reader says of it are the core's (`unread`, `crates/core/src/social.rs`).
 * Beside it, and not in its count, the groups' messages not read yet,
 * which the Community section's icon carries as a bubble of its own.
 *
 * The app reads it when the site opens, when it comes back to the front
 * and every few minutes while it is in front (`App.keepInStep`); a page
 * that shows the notifications or the alarms says so with `seen`, which
 * moves the mark and answers the count after it.
 */

import type { CoreClient, Unread } from '#lib/core/index.ts';

/** How often the count is read while a page of the site is in front, in milliseconds. */
export const UNREAD_EVERY_MS = 3 * 60_000;

/**
 * The cursor that leaves a list's mark where it is: the mark only moves
 * forward, and every cursor is after it.
 */
export const LEFT_AS_IT_IS = '0';

export class UnreadModel {
	/** What the bubble says, "3" or "99+"; nothing when nothing is new or signed out. */
	badge = $state<string>();
	/** What a screen reader says of it, "3 new". */
	label = $state<string>();
	/** New notifications, for their row on the account's page. */
	notifications = $state(0);
	/** New lines of what the alarms fired, for the row of the alarms. */
	alarms = $state(0);
	/**
	 * The groups' marker, beside this one and not in its count: messages of
	 * others in the pilot's groups not read yet, "3" or "99+"; nothing for none.
	 */
	groupsBadge = $state<string>();
	/** How many messages of others in the pilot's groups are not read yet. */
	groups = $state(0);

	#core: Pick<CoreClient, 'unread' | 'unreadSeen'>;
	#reading: Promise<void> | undefined;
	/** Counts the answers asked for, so that an older one never replaces a newer. */
	#asked = 0;

	constructor(core: Pick<CoreClient, 'unread' | 'unreadSeen'>) {
		this.#core = core;
	}

	/** Reads the count anew. Quiet: offline, the bubble keeps what it said. One at a time. */
	read(): Promise<void> {
		this.#reading ??= this.#take(() => this.#core.unread()).finally(
			() => (this.#reading = undefined)
		);
		return this.#reading;
	}

	/**
	 * The pilot was shown the lists up to these cursors: `notifications`
	 * the cursor of the notifications' answer, `alarms` that of the alarms'
	 * fetch, each left out for up to the newest and `LEFT_AS_IT_IS` for
	 * not seen. The answer is the count after it.
	 */
	async seen(lists: { notifications?: string; alarms?: string }): Promise<void> {
		await this.#reading;
		await this.#take(() => this.#core.unreadSeen(lists));
	}

	/** Signed out: no bubble. */
	clear(): void {
		this.#asked += 1;
		this.#show(undefined);
	}

	async #take(ask: () => Promise<Unread | undefined>): Promise<void> {
		const asking = (this.#asked += 1);
		try {
			const found = await ask();
			if (asking === this.#asked) this.#show(found);
		} catch {
			// Offline or refused: the bubble keeps what it said.
		}
	}

	#show(found: Unread | undefined): void {
		this.badge = found?.badge;
		this.label = found?.label;
		this.notifications = found?.notifications ?? 0;
		this.alarms = found?.alarms ?? 0;
		this.groupsBadge = found?.groupsBadge;
		this.groups = found?.groups ?? 0;
	}
}
