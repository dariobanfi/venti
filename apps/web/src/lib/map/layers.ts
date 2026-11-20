/**
 * The layers of the map tab that its chips switch on and off, as the design
 * pack's map screen has them, and how the browser remembers the pilot's
 * choice. It is a convenience of this browser: kept in `localStorage`, and
 * all layers are on wherever that cannot be read.
 */

export type MapLayer = 'wind' | 'airspace' | 'sites' | 'pilots';

export const MAP_LAYERS: readonly { layer: MapLayer; name: string }[] = [
	{ layer: 'wind', name: 'Wind' },
	{ layer: 'airspace', name: 'Airspace' },
	{ layer: 'sites', name: 'Sites' },
	{ layer: 'pilots', name: 'Pilots' }
];

const KEY = 'venti.map-layers-off';
const AERIAL_KEY = 'venti.map-aerial';

/**
 * Whether the pilot looks at the aerial view instead of the drawn ground,
 * as this browser remembers it. One choice for every map of the app.
 */
export function aerialOn(storage: Pick<Storage, 'getItem'> | undefined): boolean {
	try {
		return storage?.getItem(AERIAL_KEY) === 'on';
	} catch {
		return false;
	}
}

/** Remembers the choice of ground. Nothing happens where the browser keeps nothing. */
export function keepAerial(storage: Pick<Storage, 'setItem'> | undefined, on: boolean): void {
	try {
		storage?.setItem(AERIAL_KEY, on ? 'on' : 'off');
	} catch {
		// A private window, or no storage: the choice lasts for the visit.
	}
}

/** The layers the pilot switched off, as this browser remembers them. */
export function layersOff(storage: Pick<Storage, 'getItem'> | undefined): MapLayer[] {
	try {
		const kept: unknown = JSON.parse(storage?.getItem(KEY) ?? '[]');
		return Array.isArray(kept)
			? MAP_LAYERS.map(({ layer }) => layer).filter((layer) => kept.includes(layer))
			: [];
	} catch {
		return [];
	}
}

/** Remembers the layers switched off. Nothing happens where the browser keeps nothing. */
export function keepLayersOff(
	storage: Pick<Storage, 'setItem'> | undefined,
	off: MapLayer[]
): void {
	try {
		storage?.setItem(KEY, JSON.stringify(off));
	} catch {
		// A private window, or no storage: the choice lasts for the visit.
	}
}

/**
 * Whose the photographs of the aerial view are, in a short line, for a map
 * whose own credits are folded behind their button (a phone's width): the
 * names alone. The providers' full wording is behind that button.
 */
export const AERIAL_CREDIT = 'Photos: basemap.at · Bayerische Vermessungsverwaltung · EOX';
