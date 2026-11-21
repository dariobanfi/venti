/**
 * Flying sites as the screens see them: the take-offs, landings, winch
 * sites and training slopes around the part of the map in view, and what
 * the pilot's filter leaves of them.
 *
 * `watch.ts` holds the rules of asking the server, `filter.ts` what the
 * pilot chose; which sites a filter leaves is the core's. This class gives
 * them the page: the core in its worker to ask, a timer that waits for the
 * map to rest, and the browser's memory of the choice.
 */

import type {
	Bounds,
	CoreClient,
	SitesFilterChoices,
	SitesFiltered,
	SiteView
} from '#lib/core/index.ts';
import {
	type Answered,
	asked,
	keepChoice,
	keptChoice,
	sameAsked,
	type SitesChoice,
	shownNow,
	type SitesShownNow,
	type WindNow
} from './filter';
import { NO_SITES, type SitesState, SitesWatch } from './watch';

/** How long the map has to rest before the server is asked, in milliseconds. */
const SETTLE_MS = 400;

export class SitesModel {
	/** The sites as last learned. */
	state = $state.raw<SitesState>(NO_SITES);

	/** The filter's choices in the core's words. Nothing until the core has said them. */
	choices = $state.raw<SitesFilterChoices>();

	/** What the pilot chose to see of the sites, as this browser remembers it. */
	choice = $state.raw<SitesChoice>(keptChoice(globalThis.localStorage));

	/** The wind now, as the map has it for "Measured now". */
	windNow = $state.raw<WindNow>({ measuredDeg: undefined });

	/** The core's last answer about what the filter leaves. */
	#answered = $state.raw<Answered>();

	/** The filter the core is asked with: the choice, and the wind now where that is chosen. */
	readonly filter = $derived(asked(this.choice, this.windNow));

	/** What the map draws and the list shows: the sites the filter leaves. */
	readonly shown: SitesShownNow = $derived(
		shownNow(this.state.sites, this.filter, this.choice, this.#answered)
	);

	/** The core's words for what the filter leaves; nothing while it is asked. */
	readonly left: SitesFiltered | undefined = $derived(this.shown.left);

	#core: CoreClient;
	#watch: SitesWatch;
	#settling: ReturnType<typeof setTimeout> | undefined;
	/** Counts the questions to the core, so that an answer overtaken by a later one is dropped. */
	#asking = 0;

	constructor(core: CoreClient) {
		this.#core = core;
		this.#watch = new SitesWatch(
			(box) => core.sites(box),
			(state) => (this.state = state)
		);
		void core.sitesFilterChoices().then(
			(choices) => (this.choices = choices),
			() => {}
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

	/** The pilot changes what the filter asks; the browser remembers it. */
	choose(changed: Partial<SitesChoice>): void {
		this.choice = { ...this.choice, ...changed };
		keepChoice(globalThis.localStorage, this.choice);
	}

	/**
	 * Asks the core what the filter leaves of the sites held, unless its
	 * last answer is for these sites and this filter. A screen that shows
	 * sites calls this whenever either changes.
	 */
	async refilter(): Promise<void> {
		const sites = this.state.sites;
		const filter = this.filter;
		const answered = this.#answered;
		if (answered?.sites === sites && sameAsked(answered.filter, filter)) return;
		const asking = ++this.#asking;
		try {
			const left = await this.#core.filterSites(sites, filter);
			if (asking === this.#asking) this.#answered = { sites, filter, left };
		} catch {
			// The core could not be asked: what was shown stays.
		}
	}
}
