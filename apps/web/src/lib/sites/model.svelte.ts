/**
 * Flying sites as the screens see them: the take-offs, landings, winch
 * sites and training slopes around the part of the map in view.
 *
 * `watch.ts` holds the rules. This class gives them the page: the core in
 * its worker to ask, and a timer that waits for the map to rest.
 */

import type { Bounds, CoreClient, SiteView } from '#lib/core/index.ts';
import { NO_SITES, type SitesState, SitesWatch } from './watch';

/** How long the map has to rest before the server is asked, in milliseconds. */
const SETTLE_MS = 400;

export class SitesModel {
	/** The sites as last learned. */
	state = $state.raw<SitesState>(NO_SITES);

	#watch: SitesWatch;
	#settling: ReturnType<typeof setTimeout> | undefined;

	constructor(core: CoreClient) {
		this.#watch = new SitesWatch(
			(box) => core.sites(box),
			(state) => (this.state = state)
		);
	}

	/** The site of that identifier, as last heard of. */
	site(id: string): SiteView | undefined {
		return this.state.sites.find((site) => site.id === id);
	}

	/** The map says which part of it is in view, and at which zoom. */
	show(region: Bounds, zoom: number): void {
		const ask = this.#watch.show(region, zoom);
		clearTimeout(this.#settling);
		// The map reports every pause of a drag; wait until it rests.
		if (ask) this.#settling = setTimeout(() => void this.#watch.load(), SETTLE_MS);
	}
}
