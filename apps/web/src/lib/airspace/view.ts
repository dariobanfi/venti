/**
 * When a map asks the core for the airspace it shows: once it has come to
 * rest outside what it already has, and when the airspace itself has
 * changed. Never for a frame, and not at all while it is zoomed so far out
 * that no airspace is drawn.
 *
 * It knows nothing about the map engine or the page: `on-map.ts` connects
 * it to both.
 */

import type { Bounds } from '#lib/core/index.ts';
import { contains, grown } from '#lib/wind/watch.ts';
import { AIRSPACE_MIN_ZOOM } from './layers';

/** How much more than the part in view is asked for, so a small drag needs no question. */
const GROWN_BY = 0.25;

export class AirspaceInView {
	/** The area whose airspace the map has, and of which state of the pilot's airspace. */
	#had: { area: Bounds; revision: number } | undefined;

	/**
	 * The area to ask the core for, or nothing when the map has what it
	 * needs. `revision` counts the changes to the pilot's airspace.
	 */
	wanted(shown: Bounds, zoom: number, revision: number): Bounds | undefined {
		if (zoom < AIRSPACE_MIN_ZOOM) return undefined;
		const had = this.#had;
		if (had && had.revision === revision && contains(had.area, shown)) return undefined;
		return grown(shown, GROWN_BY);
	}

	/** The map now draws the airspace of this area, as of this state. */
	drawn(area: Bounds, revision: number): void {
		this.#had = { area, revision };
	}
}
