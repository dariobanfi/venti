/**
 * The pilot's groups and their plans for a flying day, as the screens see
 * them.
 *
 * Nothing of it is kept in the browser: the board is asked for when a
 * screen opens and after every change. The core puts the requests together
 * and reads the answers, and the worker sends them with the account's
 * token (`core/groups-source.ts`). `board.ts` holds the rules of asking;
 * this class gives them the page and what the screens watch, and puts what
 * went wrong into words. It is the browser's counterpart of the iPhone
 * app's `GroupsModel.swift`.
 */

import { untrack } from 'svelte';
import type { AccountModel } from '#lib/account/model.svelte.ts';
import type {
	CoreClient,
	GroupDetail,
	GroupJoined,
	GroupMade,
	GroupView,
	PlanBoard,
	PlanDraft,
	PlanView
} from '#lib/core/index.ts';
import { type BoardState, BoardWatch, words } from './board';

const NO_GROUPS: readonly GroupView[] = [];

export class GroupsModel {
	#state: BoardState;
	#watch: BoardWatch;
	#core: CoreClient;
	#account: AccountModel;

	constructor(core: CoreClient, account: AccountModel) {
		this.#core = core;
		this.#account = account;
		this.#watch = new BoardWatch(
			(fromDay) => core.planBoard(fromDay),
			() => (account.signedIn ? account.account?.handle : undefined),
			(state) => (this.#state = state)
		);
		this.#state = $state.raw(this.#watch.state);
	}

	/**
	 * The plans of the pilot's groups from today on, and the groups.
	 * Nothing until the server has answered once, and while signed out.
	 */
	get board(): PlanBoard | undefined {
		return this.#state.board;
	}

	/** Why the board is not there or not current, in words for the pilot. */
	get problem(): string | undefined {
		return this.#state.problem;
	}

	/**
	 * The pilot's own today, written `2026-11-21`: the day the board was
	 * asked from, by which its days are called "Today" and "Tomorrow".
	 */
	get today(): string {
		return this.#state.today;
	}

	/** The groups the pilot is in. */
	get groups(): readonly GroupView[] {
		return this.board?.groups ?? NO_GROUPS;
	}

	/** The plan with an identifier, as the board has it now. */
	plan(id: string): PlanView | undefined {
		for (const day of this.board?.days ?? []) {
			const found = day.plans.find((plan) => plan.id === id);
			if (found) return found;
		}
		return undefined;
	}

	/** Asks for the board: every plan of the pilot's groups from today on. */
	load(): Promise<void> {
		return untrack(() => this.#watch.load());
	}

	/**
	 * Keeps the board current for as long as a screen shows it: asked for
	 * now, again when the pilot signs in or out, and when the tab comes
	 * back to the front. Nothing notifies, so that is when a plan another
	 * member posted is seen. For a screen's effect; returns what to call
	 * when the screen goes away.
	 */
	keepCurrent(): () => void {
		// Read here, so that the effect runs again when the account changes.
		void this.#account.signedIn;
		void this.#account.account?.handle;
		void this.load();
		const back = () => {
			if (document.visibilityState === 'visible') void this.load();
		};
		document.addEventListener('visibilitychange', back);
		return () => document.removeEventListener('visibilitychange', back);
	}

	// Groups. Each of these throws an error whose message is in words for
	// the pilot, and asks for the board again once it has worked.

	/** Makes a group, kept by the pilot. The answer has its invite code, this once. */
	make(name: string): Promise<GroupMade> {
		return this.#change(() => this.#core.makeGroup(name));
	}

	/** Takes the pilot into the group an invite code belongs to. */
	join(code: string): Promise<GroupJoined> {
		return this.#change(() => this.#core.joinGroup(code));
	}

	/** One group with its members. */
	detail(id: string): Promise<GroupDetail> {
		return inWords(() => this.#core.group(id));
	}

	rename(id: string, name: string): Promise<GroupDetail> {
		return this.#change(() => this.#core.renameGroup(id, name));
	}

	/** Hands the keeping of a group to the member of that handle. */
	handOver(id: string, handle: string): Promise<GroupDetail> {
		return this.#change(() => this.#core.handOverGroup(id, handle));
	}

	/** A new invite code for a group. The one before it stops working. */
	newInvite(id: string): Promise<string> {
		return inWords(() => this.#core.newGroupInvite(id));
	}

	/** Removes a member from a group, which also ends its invite code. */
	remove(id: string, handle: string): Promise<void> {
		return this.#change(() => this.#core.removeGroupMember(id, handle));
	}

	leave(id: string): Promise<void> {
		return this.#change(() => this.#core.leaveGroup(id));
	}

	delete(id: string): Promise<void> {
		return this.#change(() => this.#core.deleteGroup(id));
	}

	// Plans.

	async post(group: string, draft: PlanDraft): Promise<void> {
		await this.#change(() => this.#core.postPlan(group, draft));
	}

	/** Changes one of the pilot's own plans to what the draft says. */
	async change(plan: PlanView, draft: PlanDraft): Promise<void> {
		await this.#change(() => this.#core.changePlan(plan.groupId, plan.id, draft));
	}

	deletePlan(plan: PlanView): Promise<void> {
		return this.#change(() => this.#core.deletePlan(plan.groupId, plan.id));
	}

	/** Says that the pilot is in on a plan, or takes that back. */
	async setIn(plan: PlanView, isIn: boolean): Promise<void> {
		await this.#change(() => this.#core.planIn(plan.groupId, plan.id, isIn));
	}

	/** Does one thing on the server, then asks for the board as it now is. */
	async #change<T>(work: () => Promise<T>): Promise<T> {
		const done = await inWords(work);
		await this.load();
		return done;
	}
}

/** Does something and, when it fails, throws why in words for the pilot. */
async function inWords<T>(work: () => Promise<T>): Promise<T> {
	try {
		return await work();
	} catch (error) {
		throw new Error(words(error), { cause: error });
	}
}
