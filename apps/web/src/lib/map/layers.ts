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
