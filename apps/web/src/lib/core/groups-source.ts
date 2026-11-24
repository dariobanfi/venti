/**
 * The pilot's groups and their plans for a flying day, on the Venti
 * server, as the worker talks to it.
 *
 * What to ask the server and what its answers mean is the core's: it hands
 * out a request and reads the answer, with every refusal put into words.
 * This file sends the requests in between, with the session token of the
 * account this browser is signed in to. Nothing is kept: the board is
 * asked for when it is looked at. It is the browser's counterpart of the
 * sending in the iPhone app's `GroupsModel.swift`.
 *
 * The server is the one this site is served from, as for the account.
 */

import { type Send, sendToThisOrigin } from './account-source';
import {
	type ChatMessage,
	type ChatPage,
	type CommunityDetail,
	type CommunityDraft,
	type CommunityInvite,
	type CommunityStanding,
	type CommunityView,
	type Coordinate,
	type FoundCommunity,
	type MessageDraft,
	type ReportReason,
	type SentGroupPicture,
	messageReportRequest,
	reportSent
} from './pkg/venti_core_wasm.js';
import type {
	Core,
	GroupDetail,
	GroupJoined,
	GroupMade,
	PlanBoard,
	PlanDraft,
	PlanView,
	ServerRequest
} from './pkg/venti_core_wasm.js';
import type { CoreSession } from './session';

/** `GET /v1/groups` with the session's token: the pilot's groups. */
const GROUPS_REQUEST: ServerRequest = {
	method: 'GET',
	path: '/v1/groups',
	body: undefined,
	contentType: undefined,
	needsToken: true
};

export class GroupsSource {
	#session: CoreSession;
	#send: Send;

	constructor(session: CoreSession, send: Send = sendToThisOrigin) {
		this.#session = session;
		this.#send = send;
	}

	/**
	 * The plans of every group the pilot is in, from a day on, with those
	 * groups. `fromDay` is written `2026-11-21` and is the pilot's own
	 * today, which the page knows and the server does not.
	 */
	board(fromDay: string): Promise<PlanBoard> {
		return this.#carry(
			(core) => core.planBoardRequest(fromDay, undefined),
			(core, status, body) => core.planBoard(status, body)
		);
	}

	/** Hands the keeping of a group to another member. Its keeper's to do. */
	handOver(id: string, handle: string): Promise<GroupDetail> {
		return this.#carry(
			(core) => core.handOverGroupRequest(id, handle),
			(core, status, body) => core.group(status, body)
		);
	}

	/** Removes a member who is no admin from a group. Its owner's and admins' to do. */
	remove(id: string, handle: string): Promise<void> {
		return this.#carry(
			(core) => core.removeGroupMemberRequest(id, handle),
			(core, status, body) => core.groupDone(status, body)
		);
	}

	/** Takes the pilot out of a group, with what the pilot put into it. */
	leave(id: string): Promise<void> {
		return this.#carry(
			(core) => core.leaveGroupRequest(id),
			(core, status, body) => core.groupDone(status, body)
		);
	}

	/** Deletes a group with everything in it. Its keeper's to do. */
	delete(id: string): Promise<void> {
		return this.#carry(
			(core) => core.deleteGroupRequest(id),
			(core, status, body) => core.groupDone(status, body)
		);
	}

	/** Posts a plan to a group. Its author is in from the start. */
	postPlan(group: string, draft: PlanDraft): Promise<PlanView> {
		return this.#carry(
			(core) => core.postPlanRequest(group, draft),
			(core, status, body) => core.plan(status, body)
		);
	}

	/** Changes a plan to what the draft says. Its author's to do. */
	changePlan(group: string, plan: string, draft: PlanDraft): Promise<PlanView> {
		return this.#carry(
			(core) => core.changePlanRequest(group, plan, draft),
			(core, status, body) => core.plan(status, body)
		);
	}

	/** Deletes a plan. Its author's and the group's keeper's to do. */
	deletePlan(group: string, plan: string): Promise<void> {
		return this.#carry(
			(core) => core.deletePlanRequest(group, plan),
			(core, status, body) => core.groupDone(status, body)
		);
	}

	/** Says that the pilot is in on a plan, or takes that back. */
	setIn(group: string, plan: string, isIn: boolean): Promise<PlanView> {
		return this.#carry(
			(core) => core.planInRequest(group, plan, isIn),
			(core, status, body) => core.plan(status, body)
		);
	}

	// Groups as communities with a chat (owner decision 26, docs/api.md
	// "Groups as communities with a chat").

	/** The pilot's groups as the groups' section shows them: unread and last message each. */
	communities(): Promise<CommunityView[]> {
		return this.#carry(
			// The core has `groups_request`, but its WebAssembly binding does
			// not hand it out yet: the same bare request, `GET /v1/groups`.
			() => GROUPS_REQUEST,
			(core, status, body) => core.communities(status, body)
		);
	}

	/** Public groups near a place, by name or place, or both. */
	find(near: Coordinate | undefined, text: string): Promise<FoundCommunity[]> {
		return this.#carry(
			(core) => core.findGroupsRequest(near?.lat, near?.lon, text),
			(core, status, body) => core.foundGroups(status, body)
		);
	}

	/** The pilot's invites into private groups. */
	invites(): Promise<CommunityInvite[]> {
		return this.#carry(
			(core) => core.groupInvitesRequest(),
			(core, status, body) => core.groupInvites(status, body)
		);
	}

	/** One group with its members and what the pilot may do in it. */
	community(id: string): Promise<CommunityDetail> {
		return this.#carry(
			(core) => core.groupRequest(id),
			(core, status, body) => core.community(status, body)
		);
	}

	/** Makes a group with the pilot as its owner; the answer names the group. */
	createCommunity(draft: CommunityDraft, isPublic: boolean): Promise<GroupMade> {
		return this.#carry(
			(core) => core.createCommunityRequest(draft, isPublic),
			(core, status, body) => core.groupMade(status, body)
		);
	}

	/** Changes a group's name, description and home. */
	changeCommunity(id: string, draft: CommunityDraft): Promise<CommunityDetail> {
		return this.#carry(
			(core) => core.changeCommunityRequest(id, draft),
			(core, status, body) => core.community(status, body)
		);
	}

	/** Makes a group public or private. Its owner's to do. */
	setPublic(id: string, isPublic: boolean): Promise<CommunityDetail> {
		return this.#carry(
			(core) => core.groupPublicRequest(id, isPublic),
			(core, status, body) => core.community(status, body)
		);
	}

	/** Takes the pilot into a public group, or one they were invited into. */
	enter(id: string): Promise<GroupJoined> {
		return this.#carry(
			(core) => core.enterGroupRequest(id),
			(core, status, body) => core.groupJoined(status, body)
		);
	}

	/** Invites a pilot by handle; the answer is a line to show. */
	invite(id: string, handle: string): Promise<string> {
		return this.#carry(
			(core) => core.inviteToGroupRequest(id, handle),
			(core, status, body) => core.groupInviteSent(status, body)
		);
	}

	/** Takes an invite back, or declines one by the pilot's own handle. */
	dropInvite(id: string, handle: string): Promise<void> {
		return this.#carry(
			(core) => core.dropGroupInviteRequest(id, handle),
			(core, status, body) => core.groupDone(status, body)
		);
	}

	/** Makes a member an admin, or an admin a member again. The owner's to do. */
	setAdmin(id: string, handle: string, admin: boolean): Promise<void> {
		return this.#carry(
			(core) => core.groupAdminRequest(id, handle, admin),
			(core, status, body) => core.groupDone(status, body)
		);
	}

	/** Gives a group its picture, cut square by the server. */
	setPicture(id: string, picture: Uint8Array, mediaType: string): Promise<CommunityDetail> {
		return this.#carry(
			(core) => core.groupOwnPictureRequest(id, picture, mediaType),
			(core, status, body) => core.community(status, body)
		);
	}

	/** Removes a group's picture. */
	removePicture(id: string): Promise<CommunityDetail> {
		return this.#carry(
			(core) => core.removeGroupOwnPictureRequest(id),
			(core, status, body) => core.community(status, body)
		);
	}

	/** The newest messages of a group, or the page before `before`. */
	messages(id: string, before?: string): Promise<ChatPage> {
		return this.#carry(
			(core) => core.messagesRequest(id, before),
			(core, status, body) => core.chatPage(status, body)
		);
	}

	/** What is new in a group since a page's `since`. */
	messagesSince(id: string, since: string): Promise<ChatPage> {
		return this.#carry(
			(core) => core.messagesSinceRequest(id, since),
			(core, status, body) => core.chatPage(status, body)
		);
	}

	/** Posts a message; Venti's links in its text become cards. */
	postMessage(id: string, draft: MessageDraft): Promise<ChatMessage> {
		return this.#carry(
			(core) => core.postMessageRequest(id, draft),
			(core, status, body) => core.chatMessage(status, body)
		);
	}

	/** Removes a message: the pilot's own, or as owner or admin anyone's. */
	removeMessage(id: string, message: string): Promise<ChatMessage> {
		return this.#carry(
			(core) => core.removeMessageRequest(id, message),
			(core, status, body) => core.chatMessage(status, body)
		);
	}

	/** Marks a group read up to a message's cursor. */
	markRead(id: string, cursor: string): Promise<CommunityStanding> {
		return this.#carry(
			(core) => core.groupReadRequest(id, cursor),
			(core, status, body) => core.groupStanding(status, body)
		);
	}

	/** Mutes a group, or unmutes it. */
	mute(id: string, muted: boolean): Promise<CommunityStanding> {
		return this.#carry(
			(core) => core.groupMuteRequest(id, muted),
			(core, status, body) => core.groupStanding(status, body)
		);
	}

	/** Sends a picture for a message; its identifier goes into the message. */
	sendPicture(id: string, picture: Uint8Array, mediaType: string): Promise<SentGroupPicture> {
		return this.#carry(
			(core) => core.sendGroupPictureRequest(id, picture, mediaType),
			(core, status, body) => core.groupPictureSent(status, body)
		);
	}

	/** Reports a message, or one of its pictures; what became of it, in a line. */
	async reportMessage(
		group: string,
		id: string,
		picture: boolean,
		reason: ReportReason,
		text: string
	): Promise<string> {
		const asked = await this.#session.ask(() =>
			messageReportRequest(group, id, picture, reason, text)
		);
		const answer = await this.#send(asked, await this.#session.token());
		return this.#session.ask(() => reportSent(answer.status, answer.body));
	}

	/**
	 * Has the core put a request together, sends it with the account's
	 * token and has the core read the answer. Throws in the core's words
	 * what the core refuses before anything is sent (signed out, a name
	 * that is empty or too long) and what the server refuses; and when the
	 * server could not be reached.
	 */
	async #carry<T>(
		request: (core: Core) => ServerRequest,
		read: (core: Core, status: number, body: Uint8Array) => T
	): Promise<T> {
		const asked = await this.#session.ask(request);
		const answer = await this.#send(asked, await this.#session.token());
		return this.#session.ask((core) => read(core, answer.status, answer.body));
	}
}
