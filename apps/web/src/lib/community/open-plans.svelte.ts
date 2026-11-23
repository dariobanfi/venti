/**
 * Open plans as the screens see them: the plans for everyone flying from
 * an area near a place for the coming days, one plan on its own page, and
 * the pilot's posting, changing, deleting and "I'm in".
 *
 * Nothing of it is kept in the browser. The core puts the requests
 * together and reads the answers, and the worker sends them
 * (`core/open-plans-source.ts`); this class holds the last answer for the
 * place asked about, lays what a write answered over it, and puts what
 * went wrong into words. Nothing notifies: the list is asked for when
 * Community opens, when the pilot signs in or out, and when the tab comes
 * back to the front.
 */

import { untrack } from 'svelte';
import type { AccountModel } from '#lib/account/model.svelte.ts';
import {
	type Coordinate,
	type CoreClient,
	isInWords,
	type OpenPlanDraft,
	type OpenPlans,
	type OpenPlanView,
	type TodayTakeoff
} from '#lib/core/index.ts';
import { dayOf } from '#lib/groups/days.ts';

export class OpenPlansModel {
	#core: CoreClient;
	#account: AccountModel;
	/** Counts the questions, so that only the newest answer is shown. */
	#asked = 0;
	#place: Coordinate | undefined;

	/** The open plans near the place last asked about. Nothing until the server has answered. */
	near = $state.raw<OpenPlans>();
	/** Why the list is not there or not current, in words. */
	problem = $state.raw<string>();
	/**
	 * What the pilot is told before posting a plan or saying "I'm in": the
	 * core's, from the last list read.
	 */
	publicWords = $state.raw<string>();
	/** The pilot's own today, written `2026-11-21`: the first day asked for. */
	today = $state('');
	/** Counts the writes that went through, for a screen that shows one plan. */
	revision = $state(0);

	constructor(core: CoreClient, account: AccountModel) {
		this.#core = core;
		this.#account = account;
	}

	/** Whether a pilot is signed in, for whom "I'm in" and posting are. */
	get signedIn(): boolean {
		return Boolean(this.#account.signedIn);
	}

	/**
	 * Keeps the list for a place current while a screen shows it: asked for
	 * now, again when the pilot signs in or out, and when the tab comes back
	 * to the front. For a screen's effect; returns what to call when the
	 * screen goes away.
	 */
	keepNear(place: Coordinate): () => void {
		// Read here, so that the effect runs again when the account changes:
		// whose a plan is and who is "You" follow it.
		void this.#account.signedIn;
		void this.#account.account?.handle;
		untrack(() => {
			if (
				this.#place &&
				(this.#place.lat !== place.lat || this.#place.lon !== place.lon) &&
				this.near
			) {
				this.near = undefined;
			}
			this.#place = place;
			void this.load();
		});
		const back = () => {
			if (document.visibilityState === 'visible') void this.load();
		};
		document.addEventListener('visibilitychange', back);
		return () => document.removeEventListener('visibilitychange', back);
	}

	/** Asks for the open plans near the place last given, from the pilot's own today on. */
	async load(): Promise<void> {
		const place = this.#place;
		if (!place) return;
		const asked = ++this.#asked;
		const today = dayOf(new Date());
		this.problem = undefined;
		try {
			const near = await this.#core.openPlansNear(place, today);
			if (asked !== this.#asked) return;
			this.near = near;
			this.today = today;
			this.publicWords = near.publicWords;
		} catch (error) {
			if (asked === this.#asked) this.problem = words(error);
		}
	}

	/** One open plan as the server has it now. */
	plan(id: string): Promise<OpenPlanView> {
		return inWords(() => this.#core.openPlan(id));
	}

	/** The take-offs around a place, nearest first, for the form's choice of a place. */
	async takeoffsNear(place: Coordinate): Promise<TodayTakeoff[]> {
		const answer = await this.#core.todayTakeoffs(place);
		return (answer?.listed.takeoffs ?? [])
			.filter((one) => one.distanceM !== undefined)
			.sort((a, b) => (a.distanceM ?? 0) - (b.distanceM ?? 0));
	}

	// Each of these throws an error whose message is in words for the pilot.

	/** Posts an open plan, which everyone can read. The list is asked for again. */
	async post(draft: OpenPlanDraft): Promise<OpenPlanView> {
		const posted = await inWords(() => this.#core.postOpenPlan(draft));
		this.#wrote();
		return posted;
	}

	/** Changes one of the pilot's own open plans to what the draft says. */
	async change(id: string, draft: OpenPlanDraft): Promise<OpenPlanView> {
		const changed = await inWords(() => this.#core.changeOpenPlan(id, draft));
		this.#wrote();
		return changed;
	}

	async delete(id: string): Promise<void> {
		await inWords(() => this.#core.deleteOpenPlan(id));
		this.#wrote();
	}

	/** Says that the pilot is in on an open plan, or takes that back; the answer is laid over the list. */
	async setIn(id: string, isIn: boolean): Promise<OpenPlanView> {
		const said = await inWords(() => this.#core.openPlanIn(id, isIn));
		const near = this.near;
		if (near) {
			this.near = {
				...near,
				days: near.days.map((day) => ({
					...day,
					plans: day.plans.map((plan) => (plan.id === said.id ? said : plan))
				}))
			};
		}
		this.revision += 1;
		return said;
	}

	#wrote(): void {
		this.revision += 1;
		void this.load();
	}
}

/** Why something about open plans did not work, for the pilot. */
export function words(error: unknown): string {
	const said = error instanceof Error ? error.message : String(error);
	// The core's refusals and a server out of reach are worded already.
	return isInWords(error) ? said : `The open plans could not be reached. ${said}`;
}

/** Does something and, when it fails, throws why in words for the pilot. */
async function inWords<T>(work: () => Promise<T>): Promise<T> {
	try {
		return await work();
	} catch (error) {
		throw new Error(words(error), { cause: error });
	}
}
