/**
 * Sites that pilots make, as the worker talks to the Venti server about
 * them: adding a site to Venti's own layer, correcting one, removing it,
 * putting a revision back, and reading one site and its history.
 *
 * Everything about it is the core's (`crates/core/src/site_edit.rs`): the
 * form and its words, the draft, the check, each request and what the
 * server's answer means, with every refusal put into words. This file
 * sends the requests in between, a write with the session token of the
 * account this browser is signed in to, as a report is sent
 * (`account-source.ts`).
 *
 * The server is the one this site is served from, as for the account.
 */

import { type Send, sendToThisOrigin } from './account-source';
import {
	type Coordinate,
	siteAddRequest,
	siteChangeRequest,
	siteDraftCheck,
	type SiteDraftFault,
	siteDraftMoved,
	siteDraftNew,
	siteDraftOf,
	type SiteDraftView,
	siteDraftWindInWords,
	siteForm,
	type SiteForm,
	siteHistory,
	type SiteHistory,
	siteHistoryPath,
	siteInTheWay,
	type SiteInTheWay,
	sitePage,
	type SitePage,
	sitePath,
	siteRemoved,
	siteRemoveRequest,
	siteRestoreRequest,
	type SiteView,
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
			/**
			 * Whether the site was changed by someone else meanwhile: the
			 * server's conflict that names no site in the way. The core says
			 * it in words only, so this is read from the answer's status.
			 */
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
		// Each page holds some revisions; a hundred pages are more than a site has.
		for (let page = 0; page < 100; page += 1) {
			const answer = await this.#get(siteHistoryPath(id, after));
			const next = siteHistory(answer.status, answer.body);
			whole = whole ? joined(whole, next) : next;
			if (next.nextAfter === undefined) break;
			after = next.nextAfter;
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
				changedMeanwhile: answer.status === 409 && !inTheWay
			};
		}
	}
}

/**
 * Two pages of a history as one. The core says what a revision changed
 * against the one before it on the same page, so the first revision of a
 * later page has no such lines; and it looks for the revision that undoes
 * the last change among the revisions of the page it reads, which the last
 * page holds.
 */
function joined(first: SiteHistory, next: SiteHistory): SiteHistory {
	return { ...next, revisions: [...first.revisions, ...next.revisions] };
}
