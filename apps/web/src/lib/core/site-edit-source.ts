/**
 * Sites of Venti's own two tiers, the partners' and the crowdsourced one,
 * as the worker talks to the Venti server about them: adding a site,
 * correcting one, removing it, putting a revision back, saying "I took off
 * here and it was good", and reading one site, its history and who
 * confirmed it.
 *
 * Everything about it is the core's (`crates/core/src/site_edit.rs`,
 * `site_offers.rs`): what may be offered to whom, the form and its words,
 * the draft, the check, each request and what the server's answer means,
 * with every refusal put into words. This file
 * sends the requests in between, a write with the session token of the
 * account this browser is signed in to, as a report is sent
 * (`account-source.ts`).
 *
 * The server is the one this site is served from, as for the account.
 */

import { type Send, sendToThisOrigin } from './account-source';
import {
	type Coordinate,
	type SiteAdding,
	siteAddRequest,
	siteChangeRequest,
	siteConfirmations,
	type SiteConfirmations,
	siteConfirmationsPath,
	siteConfirmed,
	type SiteConfirmFlight,
	siteConfirmForm,
	type SiteConfirmForm,
	siteDraftCheck,
	type SiteDraftFault,
	siteDraftMoved,
	siteDraftNew,
	siteDraftOf,
	siteDraftRebased,
	type SiteDraftRebased,
	type SiteDraftView,
	siteDraftWindInWords,
	siteForm,
	type SiteForm,
	type SiteHistory,
	siteHistoryPage,
	siteHistoryPath,
	siteInTheWay,
	type SiteInTheWay,
	type SiteOffers,
	sitePage,
	type SitePage,
	sitePath,
	siteRemoved,
	siteRemoveRequest,
	siteRestoreRequest,
	type SiteRevisionView,
	type SiteView,
	siteWasChanged,
	siteWithdrawn,
	siteWithdrawRequest,
	siteWords,
	type SiteWords,
	siteWritten,
	type SiteWritten
} from './pkg/venti_core_wasm.js';
import { unreachable } from './protocol';
import type { CoreSession } from './session';
import type { Get } from './wind-source';

/** What the core finds wrong with a draft, and its wind in words for the line under the rose. */
export interface SiteDraftChecked {
	faults: SiteDraftFault[];
	/** The ticked points of the compass as the core will send them: "SW to NW". */
	windInWords: string;
}

/** What became of a site that was sent: written, or refused in the core's words. */
export type SiteSent =
	| ({ kind: 'written' } & SiteWritten)
	| {
			kind: 'refused';
			/** Why, in the core's words. */
			words: string;
			/** The site that stands at the place already, to open instead. */
			inTheWay: SiteInTheWay | undefined;
			/** Whether the site was changed by someone else meanwhile, as the core reads the answer. */
			changedMeanwhile: boolean;
	  };

/**
 * Fetches from this site's origin past the browser's cache, which would
 * hand back a site as it stood before the pilot changed it.
 */
const getFresh: Get = async (path) => {
	try {
		const response = await fetch(path, {
			headers: { accept: 'application/json' },
			cache: 'no-cache',
			signal: AbortSignal.timeout(20_000)
		});
		return { status: response.status, body: new Uint8Array(await response.arrayBuffer()) };
	} catch {
		throw unreachable('Venti could not be reached for the site.');
	}
};

export class SiteEditSource {
	#session: CoreSession;
	#send: Send;
	#get: Get;
	/** Called after a write the server took: the lists of sites it answered before are old. */
	#written: () => void;

	constructor(
		session: CoreSession,
		written: () => void = () => {},
		send: Send = sendToThisOrigin,
		get: Get = getFresh
	) {
		this.#session = session;
		this.#written = written;
		this.#send = send;
		this.#get = get;
	}

	/** The form in the core's words. Asks no server. */
	form(): SiteForm {
		return siteForm();
	}

	/** The words of a site's page, the core's. Asks no server. */
	words(): SiteWords {
		return siteWords();
	}

	/**
	 * What may be offered for a site to whoever is signed in here, as the
	 * core's one answer. `takes` is what the server's status says.
	 */
	offers(
		site: SiteView,
		takes: boolean,
		confirmations: SiteConfirmations | undefined
	): Promise<SiteOffers> {
		return this.#session.ask((core) => core.siteOffers(site, takes, confirmations));
	}

	/** Whether to offer adding a site, into which tier, and the words of the entry and its form. */
	adding(takes: boolean): Promise<SiteAdding> {
		return this.#session.ask((core) => core.siteAdding(takes));
	}

	/** The form of "I took off here and it was good", for the pilot's today (`YYYY-MM-DD`). */
	confirmForm(today: string): SiteConfirmForm {
		return siteConfirmForm(today);
	}

	/** The pilot's own flights of a day that began at the site: often none. */
	confirmFlights(site: SiteView, day: string, utcOffsetS: number): Promise<SiteConfirmFlight[]> {
		return this.#session.ask((core) => core.siteConfirmFlights(site, day, utcOffsetS));
	}

	/**
	 * Says of a site "I took off here and it was good", for a day. Throws,
	 * in the core's words, before the server is asked and when it refuses.
	 */
	async confirm(
		id: string,
		day: string,
		today: string,
		note: string,
		flight: string | undefined
	): Promise<SiteWritten> {
		const request = await this.#session.ask((core) =>
			core.siteConfirmRequest(id, day, today, note, flight)
		);
		const answer = await this.#send(request, await this.#session.token());
		const written = siteConfirmed(answer.status, answer.body, day);
		this.#written();
		return written;
	}

	/** Takes the pilot's confirmation of a site back. Answers what became of it, in a line. */
	async withdraw(id: string): Promise<string> {
		const answer = await this.#send(siteWithdrawRequest(id), await this.#session.token());
		const words = siteWithdrawn(answer.status, answer.body);
		this.#written();
		return words;
	}

	/** Who says they took off at a site, the newest first, in the core's lines. */
	async confirmations(id: string): Promise<SiteConfirmations> {
		const answer = await this.#get(siteConfirmationsPath(id));
		return siteConfirmations(answer.status, answer.body);
	}

	/**
	 * A draft carried onto the site as it stands, after someone else
	 * changed it: the core's one merge.
	 */
	rebased(base: SiteView, edited: SiteDraftView, now: SiteView): SiteDraftRebased {
		return siteDraftRebased(base, edited, now);
	}

	/** An empty draft at a place. */
	draftNew(place: Coordinate): SiteDraftView {
		return siteDraftNew(place);
	}

	/** The draft that corrects a site. Throws, in the core's words, for a site of another source. */
	draftOf(site: SiteView): SiteDraftView {
		return siteDraftOf(site);
	}

	/** The draft at another place. */
	draftMoved(draft: SiteDraftView, place: Coordinate): SiteDraftView {
		return siteDraftMoved(draft, place);
	}

	/** What is wrong with a draft, field by field, and its wind in words. */
	check(draft: SiteDraftView, comment: string): SiteDraftChecked {
		return {
			faults: siteDraftCheck(draft, comment),
			windInWords: siteDraftWindInWords(new Float64Array(draft.windFromDeg))
		};
	}

	/** Adds a site for everyone. */
	async add(draft: SiteDraftView, comment: string): Promise<SiteSent> {
		return this.#write(siteAddRequest(draft, comment));
	}

	/** Saves a correction for everyone. */
	async change(draft: SiteDraftView, comment: string): Promise<SiteSent> {
		return this.#write(siteChangeRequest(draft, comment));
	}

	/**
	 * Removes a site from the map; its history stays. Answers in a line
	 * what became of it. Throws, in the core's words, when the server refuses.
	 */
	async remove(id: string, comment: string): Promise<string> {
		const request = siteRemoveRequest(id, comment);
		const answer = await this.#send(request, await this.#session.token());
		const words = siteRemoved(answer.status, answer.body);
		this.#written();
		return words;
	}

	/**
	 * Puts a site back to what one of its revisions said: how a change is
	 * undone, and how a removed site comes back. Throws, in the core's
	 * words, when the server refuses.
	 */
	async restore(id: string, revision: number, comment: string): Promise<SiteWritten> {
		const request = siteRestoreRequest(id, revision, comment);
		const answer = await this.#send(request, await this.#session.token());
		const written = siteWritten(answer.status, answer.body);
		this.#written();
		return written;
	}

	/** One site as it stands, by its identifier. */
	async page(id: string): Promise<SitePage> {
		const answer = await this.#get(sitePath(id));
		return sitePage(answer.status, answer.body);
	}

	/**
	 * The history of a site that pilots made, the first revision first:
	 * every page of it, since a site has few revisions and the screen shows
	 * the newest first.
	 */
	async history(id: string): Promise<SiteHistory> {
		let after: number | undefined;
		let whole: SiteHistory | undefined;
		// The last revision of the page before, so that the core says what
		// the first one of the next changed.
		let before: SiteRevisionView | undefined;
		// Each page holds some revisions; a hundred pages are more than a site has.
		for (let page = 0; page < 100; page += 1) {
			const answer = await this.#get(siteHistoryPath(id, after));
			const next = siteHistoryPage(answer.status, answer.body, before);
			whole = whole ? joined(whole, next) : next;
			if (next.nextAfter === undefined) break;
			after = next.nextAfter;
			before = next.revisions.at(-1);
		}
		return whole!;
	}

	async #write(request: ReturnType<typeof siteAddRequest>): Promise<SiteSent> {
		const answer = await this.#send(request, await this.#session.token());
		try {
			const written = siteWritten(answer.status, answer.body);
			this.#written();
			return { kind: 'written', ...written };
		} catch (error) {
			const inTheWay = siteInTheWay(answer.status, answer.body);
			return {
				kind: 'refused',
				words: error instanceof Error ? error.message : String(error),
				inTheWay,
				changedMeanwhile: siteWasChanged(answer.status, answer.body)
			};
		}
	}
}

/**
 * Two pages of a history as one. The core looks for the revision that
 * undoes the last change, and the one that brings a removed site back,
 * among the revisions of the page it reads, which the last page holds.
 */
function joined(first: SiteHistory, next: SiteHistory): SiteHistory {
	return { ...next, revisions: [...first.revisions, ...next.revisions] };
}
