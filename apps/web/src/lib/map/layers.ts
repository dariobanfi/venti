/**
 * The layers of the map tab, what it shows over the ground, and how the
 * browser remembers the pilot's choice. It is a convenience of this browser: kept in `localStorage`, and
 * all layers but airspace are on wherever that cannot be read. Airspace is
 * off until the pilot switches it on (the owner, 2026-11-22), and kept
 * apart from the others, as "on".
 */

export type MapLayer = 'wind' | 'airspace' | 'cables' | 'sites' | 'pilots';

export const MAP_LAYERS: readonly { layer: MapLayer; name: string }[] = [
	{ layer: 'wind', name: 'Wind' },
	{ layer: 'airspace', name: 'Airspace' },
	{ layer: 'cables', name: 'Cables' },
	{ layer: 'sites', name: 'Sites' },
	{ layer: 'pilots', name: 'Pilots' }
];

const KEY = 'venti.map-layers-off';
const AIRSPACE_KEY = 'venti.map-airspace';
const AERIAL_KEY = 'venti.map-aerial';
const WHAT_KEY = 'venti.map-what';
const MY_FLIGHTS_KEY = 'venti.map-my-flights';
const THERMAL_MAP_KEY = 'venti.map-thermal-map';
const SLOPES_KEY = 'venti.map-slopes';
const CANDIDATES_KEY = 'venti.map-candidates';
const FORECAST_LAYER_KEY = 'venti.map-forecast-layer';
const FORECAST_HEIGHT_KEY = 'venti.map-forecast-height';

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

/**
 * What the map shows over the ground, one at a time (the row along the
 * map's top): the wind, the thermals, the rain or the cloud, each along
 * one line of time from what was measured to what is forecast. The wind
 * until the pilot chooses; nothing, where the pilot pressed the one chosen
 * again.
 */
export type What = 'wind' | 'thermals' | 'rain' | 'cloud';

export const WHATS: readonly What[] = ['wind', 'thermals', 'rain', 'cloud'];

/** What the map shows, as this browser remembers it: the wind until the pilot chooses. */
export function whatChosen(storage: Pick<Storage, 'getItem'> | undefined): What | undefined {
	try {
		const kept = storage?.getItem(WHAT_KEY);
		if (kept === 'none') return undefined;
		return WHATS.find((what) => what === kept) ?? 'wind';
	} catch {
		return 'wind';
	}
}

/** Remembers what the map shows. Nothing happens where the browser keeps nothing. */
export function keepWhat(
	storage: Pick<Storage, 'setItem'> | undefined,
	what: What | undefined
): void {
	try {
		storage?.setItem(WHAT_KEY, what ?? 'none');
	} catch {
		// A private window, or no storage: the choice lasts for the visit.
	}
}

/**
 * Whether the map draws the pilot's own flights, as this browser remembers
 * it: off until the pilot switches it on (the owner, 2026-11-22).
 */
export function myFlightsOn(storage: Pick<Storage, 'getItem'> | undefined): boolean {
	try {
		return storage?.getItem(MY_FLIGHTS_KEY) === 'on';
	} catch {
		return false;
	}
}

/** Remembers whether the pilot's own flights are on. Nothing happens where the browser keeps nothing. */
export function keepMyFlights(storage: Pick<Storage, 'setItem'> | undefined, on: boolean): void {
	try {
		storage?.setItem(MY_FLIGHTS_KEY, on ? 'on' : 'off');
	} catch {
		// A private window, or no storage: the choice lasts for the visit.
	}
}

/**
 * Whether the map shows the thermal map of thermal.kk7.ch, where thermals
 * were flown in years of shared flights, as this browser remembers it: off
 * until the pilot switches it on.
 */
export function thermalMapOn(storage: Pick<Storage, 'getItem'> | undefined): boolean {
	try {
		return storage?.getItem(THERMAL_MAP_KEY) === 'on';
	} catch {
		return false;
	}
}

/** Remembers whether the thermal map is on. Nothing happens where the browser keeps nothing. */
export function keepThermalMap(storage: Pick<Storage, 'setItem'> | undefined, on: boolean): void {
	try {
		storage?.setItem(THERMAL_MAP_KEY, on ? 'on' : 'off');
	} catch {
		// A private window, or no storage: the choice lasts for the visit.
	}
}

/**
 * Whether the map shows how steep the ground is (the "Slopes" chip), as
 * this browser remembers it. Off until the pilot switches it on.
 */
export function slopesOn(storage: Pick<Storage, 'getItem'> | undefined): boolean {
	try {
		return storage?.getItem(SLOPES_KEY) === 'on';
	} catch {
		return false;
	}
}

/** Remembers whether the "Slopes" layer is on. Nothing happens where the browser keeps nothing. */
export function keepSlopes(storage: Pick<Storage, 'setItem'> | undefined, on: boolean): void {
	try {
		storage?.setItem(SLOPES_KEY, on ? 'on' : 'off');
	} catch {
		// A private window, or no storage: the choice lasts for the visit.
	}
}

/**
 * Whether the map shows the take-offs without a site (the switch beside
 * "Places from pilots" in the sites' filter), as this browser remembers
 * it. Off until the pilot switches it on.
 */
export function candidatesOn(storage: Pick<Storage, 'getItem'> | undefined): boolean {
	try {
		return storage?.getItem(CANDIDATES_KEY) === 'on';
	} catch {
		return false;
	}
}

/** Remembers whether the take-offs without a site are on. Nothing happens where the browser keeps nothing. */
export function keepCandidates(storage: Pick<Storage, 'setItem'> | undefined, on: boolean): void {
	try {
		storage?.setItem(CANDIDATES_KEY, on ? 'on' : 'off');
	} catch {
		// A private window, or no storage: the choice lasts for the visit.
	}
}

/**
 * Whether one of the two layers of what many gliders say together is on
 * ("Climbs now", "Where thermals have been"; rows of the layers' sheet),
 * as this browser remembers it. Off until the pilot switches it on.
 */
export function airLayerOn(
	storage: Pick<Storage, 'getItem'> | undefined,
	layer: 'climbs' | 'flown'
): boolean {
	try {
		return storage?.getItem(`venti.map-${layer}`) === 'on';
	} catch {
		return false;
	}
}

/** Remembers whether such a layer is on. Nothing happens where the browser keeps nothing. */
export function keepAirLayer(
	storage: Pick<Storage, 'setItem'> | undefined,
	layer: 'climbs' | 'flown',
	on: boolean
): void {
	try {
		storage?.setItem(`venti.map-${layer}`, on ? 'on' : 'off');
	} catch {
		// A private window, or no storage: the choice lasts for the visit.
	}
}

/**
 * Whether the map tab draws airspace, as this browser remembers it. Off
 * until the pilot switches it on, with the round button or the layers' row.
 */
export function airspaceOn(storage: Pick<Storage, 'getItem'> | undefined): boolean {
	try {
		return storage?.getItem(AIRSPACE_KEY) === 'on';
	} catch {
		return false;
	}
}

/** Remembers whether airspace is on. Nothing happens where the browser keeps nothing. */
export function keepAirspace(storage: Pick<Storage, 'setItem'> | undefined, on: boolean): void {
	try {
		storage?.setItem(AIRSPACE_KEY, on ? 'on' : 'off');
	} catch {
		// A private window, or no storage: the choice lasts for the visit.
	}
}

/**
 * The layers the pilot switched off, as this browser remembers them.
 * Never airspace, which is kept apart (`airspaceOn`).
 */
export function layersOff(storage: Pick<Storage, 'getItem'> | undefined): MapLayer[] {
	try {
		const kept: unknown = JSON.parse(storage?.getItem(KEY) ?? '[]');
		return Array.isArray(kept)
			? MAP_LAYERS.map(({ layer }) => layer).filter(
					(layer) => layer !== 'airspace' && kept.includes(layer)
				)
			: [];
	} catch {
		return [];
	}
}

/** Remembers the layers switched off, airspace aside. Nothing happens where the browser keeps nothing. */
export function keepLayersOff(
	storage: Pick<Storage, 'setItem'> | undefined,
	off: MapLayer[]
): void {
	try {
		storage?.setItem(KEY, JSON.stringify(off.filter((layer) => layer !== 'airspace')));
	} catch {
		// A private window, or no storage: the choice lasts for the visit.
	}
}

/**
 * Which layer of the forecast the map shows and at which height the wind, as this
 * browser remembers it: the layer by the core's name ("thermals", "wind",
 * …), the height in metres, nothing for the ground. Thermals at the ground
 * until the pilot chooses; the hour is never kept.
 */
export interface ForecastChoice {
	layer: string;
	heightM: number | undefined;
}

export function forecastChoice(storage: Pick<Storage, 'getItem'> | undefined): ForecastChoice {
	try {
		const height = Number(storage?.getItem(FORECAST_HEIGHT_KEY) ?? undefined);
		return {
			layer: storage?.getItem(FORECAST_LAYER_KEY) ?? 'thermals',
			heightM: Number.isFinite(height) && height > 0 ? height : undefined
		};
	} catch {
		return { layer: 'thermals', heightM: undefined };
	}
}

/** Remembers the layer and the height. Nothing happens where the browser keeps nothing. */
export function keepForecastChoice(
	storage: Pick<Storage, 'setItem' | 'removeItem'> | undefined,
	choice: ForecastChoice
): void {
	try {
		storage?.setItem(FORECAST_LAYER_KEY, choice.layer);
		if (choice.heightM === undefined) storage?.removeItem(FORECAST_HEIGHT_KEY);
		else storage?.setItem(FORECAST_HEIGHT_KEY, String(choice.heightM));
	} catch {
		// A private window, or no storage: the choice lasts for the visit.
	}
}
