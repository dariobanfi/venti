/**
 * One group's chat while its page is open: the newest page first, older
 * pages when the pilot scrolls up, and what is new every five seconds
 * while the page is in front (docs/api.md: there is no stream). The
 * messages are marked read up to the newest shown while the page is in
 * front. The requests and their answers are the core's; `chat.ts` holds
 * the rules of putting pages together.
 */

import type { ChatMessage, CoreClient, MessageDraft, SentGroupPicture } from '#lib/core/index.ts';
import { CHAT_EVERY_MS, cursorBefore, merged, newestCursor } from './chat';

export type ChatCore = Pick<
	CoreClient,
	| 'messages'
	| 'messagesSince'
	| 'postMessage'
	| 'removeMessage'
	| 'markCommunityRead'
	| 'sendCommunityPicture'
>;

/** Whether the page is in front, where a browser says so. */
const inFront = () => typeof document === 'undefined' || document.visibilityState === 'visible';

export class ChatModel {
	/** The messages, the oldest first. */
	messages = $state.raw<ChatMessage[]>([]);
	/** Whether the first page has come. */
	loaded = $state(false);
	/** Whether there are messages before the first shown. */
	hasOlder = $state(false);
	loadingOlder = $state(false);
	/** Why the chat could not be read, in words. */
	problem = $state<string>();

	#core: ChatCore;
	#group: string;
	#every: number;
	/** Told when the chat was marked read, so that the marker is read anew. */
	#onRead: () => void;
	#older: string | undefined;
	#since: string | undefined;
	#asking = false;
	#readUpTo: string | undefined;
	#open = false;
	/** The first page while it is being read. */
	#starting: Promise<void> | undefined;

	constructor(core: ChatCore, group: string, onRead: () => void = () => {}, every = CHAT_EVERY_MS) {
		this.#core = core;
		this.#group = group;
		this.#onRead = onRead;
		this.#every = every;
	}

	/**
	 * Reads the newest page and asks what is new every few seconds while
	 * the page is in front. For a page's effect; returns what stops it.
	 */
	open(): () => void {
		this.#open = true;
		this.#starting = this.#first();
		const timer = setInterval(() => {
			if (inFront()) void this.refresh();
		}, this.#every);
		const back = () => {
			if (inFront()) void this.refresh();
		};
		globalThis.document?.addEventListener('visibilitychange', back);
		return () => {
			this.#open = false;
			clearInterval(timer);
			globalThis.document?.removeEventListener('visibilitychange', back);
		};
	}

	async #first(): Promise<void> {
		try {
			const page = await this.#core.messages(this.#group);
			this.messages = merged(this.messages, page.messages);
			this.#older = page.older;
			this.hasOlder = page.older !== undefined;
			this.#since = page.since;
			this.problem = undefined;
			this.loaded = true;
			await this.#markRead();
		} catch (error) {
			this.problem = said(error);
		}
	}

	/** The page before the oldest shown, when the pilot scrolls up to it. */
	async loadOlder(): Promise<void> {
		if (!this.#older || this.loadingOlder) return;
		this.loadingOlder = true;
		try {
			const page = await this.#core.messages(this.#group, this.#older);
			this.messages = merged(this.messages, page.messages);
			this.#older = page.older;
			this.hasOlder = page.older !== undefined;
		} catch (error) {
			this.problem = said(error);
		} finally {
			this.loadingOlder = false;
		}
	}

	/** Asks what is new since the last answer; again at once while the server says there is more. */
	async refresh(): Promise<void> {
		await this.#starting;
		if (!this.loaded) {
			this.#starting = this.#first();
			return this.#starting;
		}
		if (this.#asking || !this.#since) return;
		this.#asking = true;
		try {
			for (let rounds = 0; rounds < 10; rounds += 1) {
				const page = await this.#core.messagesSince(this.#group, this.#since);
				this.messages = merged(this.messages, page.messages);
				this.#since = page.since;
				if (!page.more) break;
			}
			this.problem = undefined;
			await this.#markRead();
		} catch (error) {
			// Offline for a moment: the chat stays as it was and asks again.
			this.problem = said(error);
		} finally {
			this.#asking = false;
		}
	}

	/** Posts a message; it is shown at once, and the next answer shows it as the others see it. */
	async post(draft: MessageDraft): Promise<void> {
		const message = await this.#core.postMessage(this.#group, draft);
		this.messages = merged(this.messages, [message]);
		// Posting moved the pilot's own mark.
		this.#readUpTo = message.cursor;
	}

	/** Removes a message: it keeps its place and loses its words. */
	async remove(message: ChatMessage): Promise<void> {
		const removed = await this.#core.removeMessage(this.#group, message.id);
		this.messages = merged(this.messages, [removed]);
	}

	/** Sends a picture for the message being written. */
	sendPicture(bytes: Uint8Array, mediaType: string): Promise<SentGroupPicture> {
		return this.#core.sendCommunityPicture(this.#group, bytes, mediaType);
	}

	/** Marks the chat read up to the newest message shown, while the page is open and in front. */
	async #markRead(): Promise<void> {
		const newest = newestCursor(this.messages);
		if (!newest || !this.#open || !inFront()) return;
		if (this.#readUpTo && !cursorBefore(this.#readUpTo, newest)) return;
		this.#readUpTo = newest;
		try {
			await this.#core.markCommunityRead(this.#group, newest);
			this.#onRead();
		} catch {
			// Marked again with the next answer.
			this.#readUpTo = undefined;
		}
	}
}

const said = (error: unknown) => (error instanceof Error ? error.message : String(error));
