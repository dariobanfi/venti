/**
 * Adding and correcting a flying site of Venti's two tiers, and saying "I
 * took off here and it was good", as the screens see it.
 *
 * What may be offered to whom, the form, its words, the draft, the check,
 * the requests and every refusal are the core's
 * (`crates/core/src/site_edit.rs`, `site_offers.rs`), asked through the
 * worker (`core/site-edit-source.ts`). This file holds what a screen
 * watches: the core's words, whether and into which tier a site may be
 * added from here, and a draft while its form is open. It keeps no rule
 * of its own about who may do what.
 */

import type {
	Coordinate,
	CoreClient,
	SiteAdding,
	SiteConfirmations,
	SiteConfirmFlight,
	SiteConfirmForm,
	SiteDraftFault,
	SiteDraftView,
	SiteField,
	SiteForm,
	SiteHistory,
	SiteInTheWay,
	SiteOffers,
	SitePage,
	SiteView,
	SiteWords,
	SiteWritten
} from '#lib/core/index.ts';

/** Why something about a site did not work, in the words it came with. */
export function said(error: unknown): string {
	return error instanceof Error ? error.message : String(error);
}

export class SiteEditsModel {
	/** The form in the core's words, once it was asked for. */
	form = $state.raw<SiteForm>();

	/** The words of a site's page, the core's, once they were asked for. */
	words = $state.raw<SiteWords>();

	/**
	 * Whether a site may be added from here, into which tier, and what the
	 * entry and its form say: the core's answer for the account this
	 * browser is signed in to. Nothing until the core has said.
	 */
	adding = $state.raw<SiteAdding>();

	/**
	 * Counts what changes the core's answers about a site: the account
	 * (signed in, signed out, what the server says of it) and what the
	 * server's status says. A screen that shows offers asks again when it
	 * has gone up.
	 */
	asked = $state(0);

	#core: CoreClient;

	constructor(core: CoreClient) {
		this.#core = core;
		void core.siteForm().then(
			(form) => (this.form = form),
			() => {}
		);
		void core.siteWords().then(
			(words) => (this.words = words),
			() => {}
		);
		void this.accountChanged();
	}

	/**
	 * The account changed, or what the server says of it: the core is
	 * asked again what may be added, and the screens what may be offered.
	 */
	async accountChanged(): Promise<void> {
		try {
			this.adding = await this.#core.siteAdding();
		} catch {
			// The core could not be asked: nothing new is offered.
		}
		this.asked += 1;
	}

	/** What may be offered for a site to whoever is signed in here: the core's one answer. */
	offers(site: SiteView, confirmations?: SiteConfirmations): Promise<SiteOffers> {
		return this.#core.siteOffers(site, confirmations);
	}

	/** Who says they took off at a site, in the core's lines. */
	confirmations(id: string): Promise<SiteConfirmations> {
		return this.#core.siteConfirmations(id);
	}

	/** The form of "I took off here and it was good", for the pilot's today (`YYYY-MM-DD`). */
	confirmForm(today: string): Promise<SiteConfirmForm> {
		return this.#core.siteConfirmForm(today);
	}

	/** The pilot's own flights of a day that began at a site: often none. */
	confirmFlights(site: SiteView, day: string, utcOffsetS: number): Promise<SiteConfirmFlight[]> {
		return this.#core.siteConfirmFlights(site, day, utcOffsetS);
	}

	/** Says of a site "I took off here and it was good". Throws in the core's words. */
	confirm(
		id: string,
		day: string,
		today: string,
		note: string,
		flight?: string
	): Promise<SiteWritten> {
		return this.#core.confirmSite(id, day, today, note, flight);
	}

	/** Takes the pilot's confirmation back. Answers in the core's line. */
	withdraw(id: string): Promise<string> {
		return this.#core.withdrawSiteConfirmation(id);
	}

	/** A draft of a new site at a place, for its form. */
	async draftAt(place: Coordinate): Promise<SiteDraft> {
		return new SiteDraft(this.#core, await this.#core.siteDraftNew(place), undefined);
	}

	/** The draft that corrects a site. Throws, in the core's words, for a site of another source. */
	async draftOf(site: SiteView): Promise<SiteDraft> {
		return new SiteDraft(this.#core, await this.#core.siteDraftOf(site), site);
	}

	/** Removes a site from the map. Answers in the core's line what became of it. */
	remove(id: string, comment: string): Promise<string> {
		return this.#core.removeSite(id, comment);
	}

	/** Puts a site back to what one of its revisions said. */
	restore(id: string, revision: number, comment: string): Promise<SiteWritten> {
		return this.#core.restoreSite(id, revision, comment);
	}

	/** One site as it stands now. */
	page(id: string): Promise<SitePage> {
		return this.#core.sitePage(id);
	}

	/** All of a site's history, the first revision first. */
	history(id: string): Promise<SiteHistory> {
		return this.#core.siteHistory(id);
	}
}

/** Why a site was not taken, in the core's words, and what to offer with them. */
export interface SiteRefusal {
	words: string;
	/** The site that stands at the place already. */
	inTheWay?: SiteInTheWay;
}

/**
 * A site while its form is open: the draft the core made, the pilot's line
 * about the change, what the core finds wrong with it, and what became of
 * sending it. It outlives the form, which goes away while the pilot moves
 * the place on the map.
 */
export class SiteDraft {
	draft: SiteDraftView;
	/** "Why": the line kept with the revision. */
	comment = $state('');
	/** The faults the core finds, shown at their fields once the pilot has tried to send. */
	faults = $state.raw<SiteDraftFault[]>([]);
	/** The ticked points of the compass in the core's words: "SW to NW". */
	windInWords = $state('');
	/** Whether the pilot has tried to send: from then on the faults stand at their fields. */
	tried = $state(false);
	sending = $state(false);
	refusal = $state.raw<SiteRefusal>();
	/**
	 * What others changed while the pilot was editing, in the core's lines:
	 * shown with the words that say so.
	 */
	changedMeanwhile = $state.raw<string[]>([]);
	/**
	 * The fields both the pilot and the others changed, where the pilot's
	 * entry was kept: marked in the form until the pilot changes something.
	 */
	clashes = $state.raw<SiteField[]>([]);
	/** The site as it stands now, after someone else changed it: to look at beside the draft. */
	standsNow = $state.raw<SiteView>();

	#core: CoreClient;
	/** The site the draft was made from, for the core's merge. Nothing for a new site. */
	#base: SiteView | undefined;
	/** Counts the checks, so that an answer overtaken by a later one is dropped. */
	#checking = 0;

	constructor(core: CoreClient, draft: SiteDraftView, base: SiteView | undefined) {
		this.#core = core;
		this.#base = base;
		this.draft = $state.raw(draft);
		void this.check();
	}

	/**
	 * The tier of the site being corrected, as the core said it of the site
	 * the draft was made from: its notice, and whether it is a place from
	 * pilots. Nothing for a new site.
	 */
	get tier(): { note: string; fromPilots: boolean } | undefined {
		const by = this.#base?.byPilots;
		return by ? { note: by.note, fromPilots: by.fromPilots ?? false } : undefined;
	}

	/** Whether the draft corrects a site that is there. */
	get corrects(): boolean {
		return this.draft.siteId !== undefined;
	}

	/** The fault at a field, while faults are shown. */
	fault(field: SiteField): string | undefined {
		if (!this.tried) return undefined;
		return this.faults.find((fault) => fault.field === field)?.words;
	}

	/** The pilot changed something the form shows. */
	set(changed: Partial<SiteDraftView>): void {
		this.draft = { ...this.draft, ...changed };
		this.refusal = undefined;
		this.clashes = [];
		void this.check();
	}

	/** Whether a field is one both the pilot and the others changed. */
	clashed(field: SiteField): boolean {
		return this.clashes.includes(field);
	}

	/** The pilot changed the line about the change. */
	say(comment: string): void {
		this.comment = comment;
		this.refusal = undefined;
		void this.check();
	}

	/** Moves the draft to another place, as the core moves it. */
	async move(place: Coordinate): Promise<void> {
		this.draft = await this.#core.siteDraftMoved($state.snapshot(this.draft), place);
		this.refusal = undefined;
		await this.check();
	}

	/** Asks the core what is wrong with the draft as it stands. */
	async check(): Promise<SiteDraftFault[]> {
		const checking = ++this.#checking;
		const checked = await this.#core.siteDraftCheck($state.snapshot(this.draft), this.comment);
		if (checking === this.#checking) {
			this.faults = checked.faults;
			this.windInWords = checked.windInWords;
		}
		return checked.faults;
	}

	/**
	 * Sends the draft, if the core finds nothing wrong with it. Answers the
	 * site as written, or nothing: then the faults stand at their fields, or
	 * the refusal over the button.
	 *
	 * When someone else changed the site meanwhile, the draft is carried
	 * onto the site as it stands by the core's one merge
	 * (`siteDraftRebased`): what the pilot changed stays, what the pilot
	 * left alone is as the site stands now, and the core's words, what the
	 * others changed and the fields both changed stand with the form, to
	 * look at before sending again.
	 */
	async send(): Promise<SiteWritten | undefined> {
		if (this.sending) return undefined;
		this.tried = true;
		this.sending = true;
		this.refusal = undefined;
		try {
			if ((await this.check()).length > 0) return undefined;
			const draft = $state.snapshot(this.draft);
			const sent = this.corrects
				? await this.#core.changeSite(draft, this.comment)
				: await this.#core.addSite(draft, this.comment);
			if (sent.kind === 'written') return { site: sent.site, words: sent.words };
			this.refusal = { words: sent.words, inTheWay: sent.inTheWay };
			if (sent.changedMeanwhile && draft.siteId) await this.#rebase(draft.siteId, draft);
			return undefined;
		} catch (error) {
			this.refusal = { words: said(error) };
			return undefined;
		} finally {
			this.sending = false;
		}
	}

	async #rebase(id: string, edited: SiteDraftView): Promise<void> {
		const base = this.#base;
		if (!base) return;
		try {
			const now = (await this.#core.sitePage(id)).site;
			const rebased = await this.#core.siteDraftRebased($state.snapshot(base), edited, now);
			this.draft = rebased.draft;
			this.#base = now;
			this.standsNow = now;
			this.changedMeanwhile = rebased.theirs;
			this.clashes = rebased.clashes;
			this.refusal = { words: rebased.words };
			await this.check();
		} catch {
			// Not to be had now: the refusal says to look at the site.
		}
	}
}
