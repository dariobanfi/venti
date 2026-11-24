/**
 * The rules of a group's chat as the browser shows it, apart from the
 * page: in which order the messages stand, and how a page of the server's
 * answer goes into what is shown. The messages, their words and their
 * cards are the core's (`chatPage`, `chatMessage`); the server gives a page
 * newest first, and what is new since an answer in the order it happened
 * (docs/api.md, "Groups as communities with a chat").
 */

import type { ChatMessage } from '#lib/core/index.ts';

/** How often a chat that is open and in front asks what is new, in milliseconds (docs/api.md). */
export const CHAT_EVERY_MS = 5_000;

/** The most pictures a message carries (docs/api.md). */
export const PICTURES_AT_MOST = 4;

/**
 * Whether one cursor stands before another. A cursor is a place in the
 * chat, written in digits; two of them compare as the numbers they are.
 */
export function cursorBefore(one: string, other: string): boolean {
	if (one.length !== other.length) return one.length < other.length;
	return one < other;
}

/**
 * The messages to show, the oldest first, after `came` arrived: a page,
 * a page from before, what is new, or a message the pilot just posted or
 * removed. A message already shown is replaced by what came of it: a
 * removed one keeps its place and loses its words.
 */
export function merged(shown: readonly ChatMessage[], came: readonly ChatMessage[]): ChatMessage[] {
	const byId = new Map(shown.map((message) => [message.id, message]));
	for (const message of came) byId.set(message.id, message);
	return [...byId.values()].toSorted((one, other) =>
		one.cursor === other.cursor ? 0 : cursorBefore(one.cursor, other.cursor) ? -1 : 1
	);
}

/** The cursor of the newest message, to mark the chat read up to; none for an empty chat. */
export function newestCursor(shown: readonly ChatMessage[]): string | undefined {
	return shown.at(-1)?.cursor;
}

/**
 * The day a message is shown under, and its time, by the browser's clock:
 * "14:05" for today's, a date before that.
 */
export function postedWords(postedAt: string, now: Date, locale?: string): string {
	const at = new Date(postedAt);
	if (Number.isNaN(at.getTime())) return '';
	const time = at.toLocaleTimeString(locale, { hour: '2-digit', minute: '2-digit' });
	if (at.toDateString() === now.toDateString()) return time;
	const day = at.toLocaleDateString(locale, { day: 'numeric', month: 'short' });
	return `${day}, ${time}`;
}
