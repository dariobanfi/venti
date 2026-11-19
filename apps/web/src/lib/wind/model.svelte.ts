/**
 * Live wind as the screens see it: the stations around the part of the map
 * in view, kept current while the map is on screen.
 *
 * `watch.ts` holds the rules. This class gives them the page: the core in
 * its worker to ask, timers, and whether the tab is in front.
 */

import type { Bounds, CoreClient, StationHistory, WindStation } from '#lib/core/index.ts';
import { NO_WIND, REFRESH_MS, type WindState, WindWatch } from './watch';

/** How long the map has to rest before the server is asked, in milliseconds. */
const SETTLE_MS = 300;

export class WindModel {
	/** The wind as last learned. */
	state = $state.raw<WindState>(NO_WIND);

	#core: CoreClient;
	#watch: WindWatch;
	#settling: ReturnType<typeof setTimeout> | undefined;

	constructor(core: CoreClient) {
		this.#core = core;
		this.#watch = new WindWatch(
			(box) => core.wind(box),
			(state) => (this.state = state)
		);
	}

	/** The station of that identifier, as last heard of. */
	station(id: string): WindStation | undefined {
		return this.state.stations.find((station) => station.id === id);
	}

	/** The map says which part of it is in view. */
	show(region: Bounds): void {
		const ask = this.#watch.show(region);
		clearTimeout(this.#settling);
		// The map reports every pause of a drag; wait until it rests.
		if (ask) this.#settling = setTimeout(() => void this.#watch.load(), SETTLE_MS);
	}

	/**
	 * Asks again every minute for as long as the map is on screen and the tab
	 * is in front. The first question is asked when the map says what it
	 * shows. Returns what to call when the map leaves the screen.
	 */
	keepCurrent(): () => void {
		const refresh = () => {
			if (document.visibilityState === 'visible' && this.#watch.due) void this.#watch.load();
		};
		const timer = setInterval(refresh, REFRESH_MS);
		// A tab that comes back to the front after a while catches up at once.
		document.addEventListener('visibilitychange', refresh);
		return () => {
			clearInterval(timer);
			clearTimeout(this.#settling);
			document.removeEventListener('visibilitychange', refresh);
		};
	}

	/** What a station measured over the last 24 hours. */
	history(stationId: string): Promise<StationHistory> {
		return this.#core.stationHistory(stationId);
	}
}
