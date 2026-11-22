/**
 * The place the home page's weather is for, as this browser remembers it:
 * the position Today or the home page last had from the browser, or the
 * take-off the pilot chose on the home page. Kept in `localStorage`, to a
 * hundredth of a degree (about a kilometre), which is all the weather
 * needs; nothing of it leaves the browser.
 */

import type { Coordinate } from '#lib/core/index.ts';

/** Where the weather is for, and what kind of place that is. */
export interface WeatherPlace {
	at: Coordinate;
	/** The browser's position, or a take-off the pilot chose, by its name. */
	kind: 'position' | 'chosen';
	/** The take-off's name, for a chosen one. */
	name?: string;
}

const KEY = 'venti.weather-place';

/** To a hundredth of a degree. */
function rounded(value: number): number {
	return Math.round(value * 100) / 100;
}

/** What the browser remembers, if anything it can read. */
export function rememberedPlace(
	storage: Storage | undefined = safeStorage()
): WeatherPlace | undefined {
	try {
		const said = JSON.parse(storage?.getItem(KEY) ?? 'null');
		if (!said || typeof said !== 'object') return undefined;
		const lat = Number(said.at?.lat);
		const lon = Number(said.at?.lon);
		if (!Number.isFinite(lat) || !Number.isFinite(lon) || Math.abs(lat) > 90 || Math.abs(lon) > 180)
			return undefined;
		const kind = said.kind === 'chosen' ? 'chosen' : 'position';
		const name = kind === 'chosen' && typeof said.name === 'string' ? said.name : undefined;
		return { at: { lat, lon }, kind, name };
	} catch {
		return undefined;
	}
}

/** Remembers a place for the next visit. */
export function rememberPlace(
	place: WeatherPlace,
	storage: Storage | undefined = safeStorage()
): WeatherPlace {
	const kept: WeatherPlace = {
		at: { lat: rounded(place.at.lat), lon: rounded(place.at.lon) },
		kind: place.kind,
		name: place.kind === 'chosen' ? place.name : undefined
	};
	try {
		storage?.setItem(KEY, JSON.stringify(kept));
	} catch {
		// A browser that keeps nothing shows the region's middle next time.
	}
	return kept;
}

function safeStorage(): Storage | undefined {
	try {
		return globalThis.localStorage;
	} catch {
		return undefined;
	}
}
