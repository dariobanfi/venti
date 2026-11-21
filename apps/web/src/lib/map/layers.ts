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
const FORECAST_KEY = 'venti.map-forecast';
const RADAR_KEY = 'venti.map-radar';
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
 * Whether the map shows the kind of day, square by square (the "Forecast"
 * chip), as this browser remembers it. Off until the pilot switches it on.
 */
export function forecastOn(storage: Pick<Storage, 'getItem'> | undefined): boolean {
	try {
		return storage?.getItem(FORECAST_KEY) === 'on';
	} catch {
		return false;
	}
}

/** Remembers whether the "Forecast" layer is on. Nothing happens where the browser keeps nothing. */
export function keepForecast(storage: Pick<Storage, 'setItem'> | undefined, on: boolean): void {
	try {
		storage?.setItem(FORECAST_KEY, on ? 'on' : 'off');
	} catch {
		// A private window, or no storage: the choice lasts for the visit.
	}
}

/**
 * Whether the map shows the rain radar (the "Radar" chip), as this browser
 * remembers it. Off until the pilot switches it on.
 */
export function radarOn(storage: Pick<Storage, 'getItem'> | undefined): boolean {
	try {
		return storage?.getItem(RADAR_KEY) === 'on';
	} catch {
		return false;
	}
}

/** Remembers whether the "Radar" layer is on. Nothing happens where the browser keeps nothing. */
export function keepRadar(storage: Pick<Storage, 'setItem'> | undefined, on: boolean): void {
	try {
		storage?.setItem(RADAR_KEY, on ? 'on' : 'off');
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
 * What the "Forecast" layer shows and at which height the wind, as this
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
