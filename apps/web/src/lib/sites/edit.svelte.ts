/**
 * Adding and correcting a flying site, as the screens see it.
 *
 * The form, its words, the draft, the check, the requests and every
 * refusal are the core's (`crates/core/src/site_edit.rs`), asked through
 * the worker (`core/site-edit-source.ts`). This file holds what a screen
 * watches: whether the server takes sites from pilots at all, and a draft
 * while its form is open.
 */

import type {
	Coordinate,
	CoreClient,
	SiteDraftFault,
	SiteDraftView,
	SiteField,
	SiteForm,
	SiteHistory,
	SiteInTheWay,
	SitePage,
	SiteRevisionView,
	SiteView,
	SiteWritten
} from '#lib/core/index.ts';

/** Why something about a site did not work, in the words it came with. */
export function said(error: unknown): string {
	return error instanceof Error ? error.message : String(error);
}

export class SiteEditsModel {
	/**
	 * Whether the server takes flying sites from pilots: whether to offer
	 * "Add a site" and "Correct this site" at all. No until it has said so.
	 */
	takes = $state(false);

	/** The form in the core's words, once it was asked for. */
	form = $state.raw<SiteForm>();

	#core: CoreClient;

	constructor(core: CoreClient) {
		this.#core = core;
		void core.takesSites().then(
			(takes) => {
				this.takes = takes;
				// The words on what opens the form are the form's.
				if (takes) void core.siteForm().then((form) => (this.form = form));
			},
			() => {}
		);
	}

	/** A draft of a new site at a place, for its form. */
	async draftAt(place: Coordinate): Promise<SiteDraft> {
		return new SiteDraft(this.#core, await this.#core.siteDraftNew(place));
	}

	/** The draft that corrects a site. Throws, in the core's words, for a site of another source. */
	async draftOf(site: SiteView): Promise<SiteDraft> {
		return new SiteDraft(this.#core, await this.#core.siteDraftOf(site));
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
	 * What others changed while the pilot was editing, the newest first, in
	 * the core's words: shown with the refusal that says so.
	 */
	changedMeanwhile = $state.raw<SiteRevisionView[]>([]);

	#core: CoreClient;
	/** Counts the checks, so that an answer overtaken by a later one is dropped. */
	#checking = 0;

	constructor(core: CoreClient, draft: SiteDraftView) {
		this.#core = core;
		this.draft = $state.raw(draft);
		void this.check();
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
		void this.check();
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
	 * When someone else changed the site meanwhile, the pilot's entries
	 * stay in the form. The draft then stands on the site's newest
	 * revision, so that it can be sent again, and what the others changed
	 * is listed with the refusal for the pilot to look at first.
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
			if (sent.changedMeanwhile && draft.siteId) await this.#standOnNewest(draft.siteId);
			return undefined;
		} catch (error) {
			this.refusal = { words: said(error) };
			return undefined;
		} finally {
			this.sending = false;
		}
	}

	async #standOnNewest(id: string): Promise<void> {
		try {
			const history = await this.#core.siteHistory(id);
			const had = this.draft.revision ?? 0;
			this.changedMeanwhile = history.revisions
				.filter((revision) => revision.number > had)
				.toReversed();
			this.draft = { ...this.draft, revision: history.revision };
		} catch {
			// Not to be had now: the refusal says to look at the site.
		}
	}
}
