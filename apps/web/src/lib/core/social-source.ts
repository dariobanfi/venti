/**
 * Pilots among each other, as the worker talks to the server about it:
 * following a pilot, the flights of the pilots one follows, cheers and
 * comments under a public flight, and what the pilot was told of them. And
 * the plumbing of Web Push for the alarms: the server's key and this
 * browser's subscription.
 *
 * Everything a pilot reads is the core's (`crates/core/src/social.rs`,
 * `push.rs`): the buttons' words, every line, the faults of a comment
 * being written, the requests, what the answers mean and every refusal.
 * This file sends the requests in between, with the session token where
 * the core asks for it. Nothing is kept: all of it is read from the server
 * when a screen shows it.
 */

import { type Send, sendToThisOrigin } from './account-source';
import {
	cheerAnswer,
	cheerList,
	type CheerList,
	cheerRequest,
	cheersRequest,
	type CheerState,
	commentDraft,
	type CommentDraft,
	commentList,
	type CommentList,
	commentPosted,
	type CommentPosted,
	commentRemoveRequest,
	commentReportForm,
	commentReportRequest,
	commentRequest,
	commentsOpenAnswer,
	commentsOpenRequest,
	commentsRequest,
	commentsSwitch,
	type CommentsSwitch,
	feed,
	type Feed,
	feedRequest,
	followAnswer,
	followersRequest,
	followList,
	type FollowList,
	followRequest,
	type FollowState,
	notifications,
	type Notifications,
	notificationsRequest,
	pushKey,
	pushKeyRequest,
	pushPermissionLine,
	pushSaved,
	pushSubscribeRequest,
	pushUnsubscribeRequest,
	type ReportForm,
	type ReportReason,
	reportSent,
	socialDone,
	socialWords,
	type SocialWords
} from './pkg/venti_core_wasm.js';
import type { CoreSession } from './session';

/** A browser's subscription to Web Push, as its `toJSON()` gives it. */
export interface PushSubscribed {
	endpoint: string;
	p256dh: string;
	auth: string;
}

export class SocialSource {
	#session: CoreSession;
	#send: Send;
	/**
	 * The current time by the server's clock, in seconds: ages are counted
	 * from it. 0 where the app has not read the server's clock, never the
	 * browser's own: the core then gives a comment's day instead of an age.
	 */
	#now: () => Promise<number>;

	constructor(
		session: CoreSession,
		now: () => Promise<number> = async () => 0,
		send: Send = sendToThisOrigin
	) {
		this.#session = session;
		this.#now = now;
		this.#send = send;
	}

	/** The words no answer carries: the feed's choice, the titles, the comment field's. */
	words(): SocialWords {
		return socialWords();
	}

	async #ask(request: Parameters<Send>[0]) {
		return this.#send(request, await this.#session.token());
	}

	async #signedIn(): Promise<boolean> {
		return (await this.#session.token()) !== undefined;
	}

	/**
	 * Who follows a pilot, with the two counts in words; signed in, also
	 * whether this pilot follows them and the button's word.
	 */
	async follows(handle: string): Promise<FollowList> {
		const answer = await this.#ask(followersRequest(handle, undefined, await this.#signedIn()));
		return followList(answer.status, answer.body);
	}

	/** Follows a pilot, or follows them no more. Throws the core's refusal. */
	async follow(handle: string, follow: boolean): Promise<FollowState> {
		const answer = await this.#ask(followRequest(handle, follow));
		return followAnswer(answer.status, answer.body);
	}

	/**
	 * One page of the public flights of the pilots this pilot follows, each
	 * with its cheers and comments in words. Nothing in a browser that is
	 * not signed in.
	 */
	async feed(before?: string): Promise<Feed | undefined> {
		if (!(await this.#signedIn())) return undefined;
		const answer = await this.#ask(feedRequest(before));
		return feed(answer.status, answer.body);
	}

	/** Who cheered a public flight, in a line, and the button's word. */
	async cheers(flightId: string): Promise<CheerList> {
		const answer = await this.#ask(cheersRequest(flightId, undefined, await this.#signedIn()));
		return cheerList(answer.status, answer.body);
	}

	/** Cheers a flight, or takes the cheer back. Throws the core's refusal. */
	async cheer(flightId: string, cheer: boolean): Promise<CheerState> {
		const answer = await this.#ask(cheerRequest(flightId, cheer));
		return cheerAnswer(answer.status, answer.body);
	}

	/**
	 * A page of the comments under a public flight, the oldest first. Each
	 * says whether this pilot may remove it: their own, and any under a
	 * flight of theirs (`flightHandle`).
	 */
	async comments(flightId: string, flightHandle?: string, after?: string): Promise<CommentList> {
		const answer = await this.#ask(commentsRequest(flightId, after));
		const account = await this.#session.ask((core) => core.account());
		return commentList(
			answer.status,
			answer.body,
			await this.#now(),
			account && !account.needsSignIn ? account.handle : undefined,
			flightHandle
		);
	}

	/** A comment while it is written: the room left, what is wrong, whether it can be sent. */
	draft(text: string): CommentDraft {
		return commentDraft(text);
	}

	/** Posts a comment under a public flight. Throws the core's refusal. */
	async comment(flightId: string, text: string): Promise<CommentPosted> {
		const answer = await this.#ask(commentRequest(flightId, text));
		return commentPosted(answer.status, answer.body, await this.#now());
	}

	/** Removes a comment: the pilot's own, or one under a flight of theirs. */
	async removeComment(flightId: string, commentId: string): Promise<void> {
		const answer = await this.#ask(commentRemoveRequest(flightId, commentId));
		socialDone(answer.status, answer.body);
	}

	/** What a report about a comment can say. */
	commentReportForm(): ReportForm {
		return commentReportForm();
	}

	/** Reports a comment to whoever runs the server; what became of it, in a line. */
	async reportComment(
		flightId: string,
		commentId: string,
		reason: ReportReason,
		text: string
	): Promise<string> {
		const answer = await this.#ask(commentReportRequest(flightId, commentId, reason, text));
		return reportSent(answer.status, answer.body);
	}

	/** The switch on the pilot's own flight: whether it takes comments, in words. */
	commentsSwitch(open: boolean): CommentsSwitch {
		return commentsSwitch(open);
	}

	/** Says whether a flight of the pilot's own takes comments. */
	async setCommentsOpen(flightId: string, open: boolean): Promise<CommentsSwitch> {
		const answer = await this.#ask(commentsOpenRequest(flightId, open));
		return commentsOpenAnswer(answer.status, answer.body);
	}

	/** What the pilot was told: who follows, cheered or wrote. Nothing when signed out. */
	async notifications(since?: string): Promise<Notifications | undefined> {
		if (!(await this.#signedIn())) return undefined;
		const answer = await this.#ask(notificationsRequest(since));
		return notifications(answer.status, answer.body, await this.#now());
	}

	/** The line beside the switch that asks for the browser's permission. */
	pushLine(): string {
		return pushPermissionLine();
	}

	/** The server's public key for Web Push: the `applicationServerKey`, base64url. */
	async pushKey(): Promise<string> {
		const answer = await this.#ask(pushKeyRequest());
		return pushKey(answer.status, answer.body);
	}

	/** Tells the server of this browser's subscription. Throws the core's refusal. */
	async pushSubscribe(subscribed: PushSubscribed): Promise<void> {
		const answer = await this.#ask(
			pushSubscribeRequest(subscribed.endpoint, subscribed.p256dh, subscribed.auth)
		);
		pushSaved(answer.status, answer.body);
	}

	/** Tells the server that this browser's subscription has ended. */
	async pushUnsubscribe(endpoint: string): Promise<void> {
		const answer = await this.#ask(pushUnsubscribeRequest(endpoint));
		pushSaved(answer.status, answer.body);
	}
}
