/**
 * The pilot's own sites as the screens see them: the take-offs a pilot
 * marks as theirs, which Today lists first.
 *
 * The core keeps them among its files in this browser and answers every
 * question about them (`Core.keepSite`, `forgetSite`, `isSiteKept`,
 * `keptSites`); nothing of them goes to the server. This class asks, and
 * counts the changes, so that a screen that shows them reads them anew.
 */

import {
	type Coordinate,
	type CoreClient,
	type Credit,
	type KeptSites,
	type SiteView
} from '#lib/core/index.ts';

export class KeptSitesModel {
	/**
	 * Counts the changes to the pilot's own sites. Today asks for its
	 * take-offs anew, and a list reads again, when it goes up.
	 */
	revision = $state(0);

	/** The pilot's own sites as last read for a list. Nothing until then. */
	list = $state.raw<KeptSites>();

	#core: CoreClient;
	/** The reading last asked for: an older one's answer is let go. */
	#reading = 0;

	constructor(core: CoreClient) {
		this.#core = core;
	}

	/** Whether a site is one of the pilot's own, for the button on its page. */
	isKept(id: string): Promise<boolean> {
		return this.#core.isSiteKept(id);
	}

	/**
	 * Marks a site as the pilot's own: the site as the core gave it, with
	 * the credits and the date of the answer it came from.
	 */
	async keep(site: SiteView, credits: Credit[], asOfUnixS: number | undefined): Promise<void> {
		await this.#core.keepSite(site, credits, asOfUnixS);
		this.revision += 1;
	}

	async forget(id: string): Promise<void> {
		await this.#core.forgetSite(id);
		this.revision += 1;
	}

	/**
	 * Reads the list: from a place the nearest first, each with its
	 * distance; without one by name.
	 */
	async read(from?: Coordinate): Promise<void> {
		const reading = ++this.#reading;
		const list = await this.#core.keptSites(from);
		if (reading === this.#reading) this.list = list;
	}

	/** Another tab kept or forgot a site: what is shown here is read again. */
	changedElsewhere(): void {
		this.revision += 1;
	}
}
