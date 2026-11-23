/**
 * Open plans on the Venti server, as the worker talks to it: a plan for
 * everyone flying from an area, "Saturday at the Wallberg, 2 seats free",
 * that anyone may read and every signed-in pilot may say "I'm in" to.
 *
 * What to ask and what the answers mean is the core's
 * (`crates/core/src/open_plans.rs`): it hands out each request and reads
 * each answer, with every refusal in its words. This file sends them in
 * between, with the session token where the core says one is needed. The
 * open plans near a place are asked for without one. Nothing is kept.
 */

import { type Send, sendToThisOrigin } from './account-source';
import type {
	Coordinate,
	Core,
	OpenPlanDraft,
	OpenPlans,
	OpenPlanView,
	ServerRequest
} from './pkg/venti_core_wasm.js';
import type { CoreSession } from './session';

export class OpenPlansSource {
	#session: CoreSession;
	#send: Send;

	constructor(session: CoreSession, send: Send = sendToThisOrigin) {
		this.#session = session;
		this.#send = send;
	}

	/**
	 * The open plans within the core's radius of a place, from a day on, by
	 * day. `fromDay` is the pilot's own today, written `2026-11-21`, which
	 * the page knows and the server does not.
	 */
	near(place: Coordinate, fromDay: string): Promise<OpenPlans> {
		return this.#carry(
			(core) => core.openPlansNearRequest(place, undefined, fromDay, undefined),
			(core, status, body) => core.openPlans(status, body)
		);
	}

	/** The open plans the pilot posted or is in on. */
	mine(): Promise<OpenPlans> {
		return this.#carry(
			(core) => core.myOpenPlansRequest(),
			(core, status, body) => core.openPlans(status, body)
		);
	}

	/** One open plan, which anyone may read. */
	plan(id: string): Promise<OpenPlanView> {
		return this.#carry(
			(core) => core.openPlanRequest(id),
			(core, status, body) => core.openPlan(status, body)
		);
	}

	/** Posts an open plan. Its author is in from the start. */
	post(draft: OpenPlanDraft): Promise<OpenPlanView> {
		return this.#carry(
			(core) => core.postOpenPlanRequest(draft),
			(core, status, body) => core.openPlan(status, body)
		);
	}

	/** Changes an open plan to what the draft says, whole. Its author's to do. */
	change(id: string, draft: OpenPlanDraft): Promise<OpenPlanView> {
		return this.#carry(
			(core) => core.changeOpenPlanRequest(id, draft),
			(core, status, body) => core.openPlan(status, body)
		);
	}

	/** Deletes an open plan. Its author's to do. */
	delete(id: string): Promise<void> {
		return this.#carry(
			(core) => core.deleteOpenPlanRequest(id),
			(core, status, body) => core.groupDone(status, body)
		);
	}

	/** Says that the pilot is in on an open plan, or takes that back. */
	setIn(id: string, isIn: boolean): Promise<OpenPlanView> {
		return this.#carry(
			(core) => core.openPlanInRequest(id, isIn),
			(core, status, body) => core.openPlan(status, body)
		);
	}

	/**
	 * Has the core put a request together, sends it, with the account's
	 * token where the core asks for one, and has the core read the answer.
	 * Throws in the core's words what the core refuses before anything is
	 * sent and what the server refuses, and when the server could not be
	 * reached.
	 */
	async #carry<T>(
		request: (core: Core) => ServerRequest,
		read: (core: Core, status: number, body: Uint8Array) => T
	): Promise<T> {
		const asked = await this.#session.ask(request);
		const token = asked.needsToken ? await this.#session.token() : undefined;
		const answer = await this.#send(asked, token);
		return this.#session.ask((core) => read(core, answer.status, answer.body));
	}
}
