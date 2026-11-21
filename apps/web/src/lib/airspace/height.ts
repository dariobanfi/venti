/**
 * The height the pilot filters the maps' airspace by, as this browser
 * remembers it, as it remembers the aerial view: in `localStorage`, and
 * all heights wherever that cannot be read. The steps and what they take
 * off the map are the core's (`airspaceHeightFilter`).
 */

const CEILING_KEY = 'venti.airspace-ceiling';

/**
 * The height the pilot filters the map's airspace by, in metres, as this
 * browser remembers it: nothing for all heights. One choice for every map
 * of the app that draws airspace.
 */
export function ceilingKept(storage: Pick<Storage, 'getItem'> | undefined): number | undefined {
	try {
		const kept = Number(storage?.getItem(CEILING_KEY) ?? undefined);
		return Number.isFinite(kept) && kept > 0 ? kept : undefined;
	} catch {
		return undefined;
	}
}

/** Remembers the height filter. Nothing happens where the browser keeps nothing. */
export function keepCeiling(
	storage: Pick<Storage, 'setItem' | 'removeItem'> | undefined,
	ceilingM: number | undefined
): void {
	try {
		if (ceilingM === undefined) storage?.removeItem(CEILING_KEY);
		else storage?.setItem(CEILING_KEY, String(ceilingM));
	} catch {
		// A private window, or no storage: the choice lasts for the visit.
	}
}
