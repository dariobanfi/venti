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

	/** Makes a group, kept by the pilot. The answer has its invite code, this once. */
	make(name: string): Promise<GroupMade> {
		return this.#carry(
			(core) => core.createGroupRequest(name),
			(core, status, body) => core.groupMade(status, body)
		);
	}

	/** Takes the pilot into the group an invite code belongs to. */
	join(code: string): Promise<GroupJoined> {
		return this.#carry(
			(core) => core.joinGroupRequest(code),
			(core, status, body) => core.groupJoined(status, body)
		);
	}

	/** One group with its members. */
	group(id: string): Promise<GroupDetail> {
		return this.#carry(
			(core) => core.groupRequest(id),
			(core, status, body) => core.group(status, body)
		);
	}

	/** Renames a group. Its keeper's to do. */
	rename(id: string, name: string): Promise<GroupDetail> {
		return this.#carry(
			(core) => core.renameGroupRequest(id, name),
			(core, status, body) => core.group(status, body)
		);
	}

	/** Hands the keeping of a group to another member. Its keeper's to do. */
	handOver(id: string, handle: string): Promise<GroupDetail> {
		return this.#carry(
			(core) => core.handOverGroupRequest(id, handle),
			(core, status, body) => core.group(status, body)
		);
	}

	/** A new invite code for a group. The one before it stops working. */
	newInvite(id: string): Promise<string> {
		return this.#carry(
			(core) => core.newGroupInviteRequest(id),
			(core, status, body) => core.groupInvite(status, body)
		);
	}

	/** Removes a member from a group, which also ends its invite code. Its keeper's to do. */
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
