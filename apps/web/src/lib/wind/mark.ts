/**
 * How one wind station is drawn on the map and read in a list: an arrow in
 * the colour of its wind, a pale dot for calm, a grey dot for a reading too
 * old to stand for the wind now. The iPhone app's `WindBand.swift` draws
 * the same marks by the same rules.
 *
 * Whether a reading is old is the core's decision (`isOld`).
 */

import type { WindStation } from '#lib/core/index.ts';
import { WIND_BANDS, windBand } from '#lib/design/palette.ts';

export type WindSymbol =
	/** An arrow in the band's colour, pointing where the wind blows. */
	| { kind: 'arrow'; band: number }
	/** Wind too weak to have a direction worth drawing. */
	| { kind: 'calm' }
	/** A reading too old to stand for the wind now, or none at all. */
	| { kind: 'old' };

export interface WindMark {
	symbol: WindSymbol;
	/**
	 * Where the arrow points, in degrees clockwise from north: the direction
	 * the wind blows towards.
	 */
	heading: number;
	/**
	 * Mean wind and gusts in km/h, "12/25", or just "12" without gusts.
	 * Empty for a station with nothing current to say.
	 */
	label: string;
}

/** Below this the wind has no direction worth an arrow, in metres per second. */
export const CALM_BELOW_MS = 0.5;

export function windMark(station: WindStation): WindMark {
	const reading = station.latest;
	if (!reading || station.isOld || reading.speedMs === undefined) {
		return { symbol: { kind: 'old' }, heading: 0, label: '' };
	}
	const label = speeds(reading.speedMs, reading.gustMs);
	if (reading.directionDeg === undefined || reading.speedMs < CALM_BELOW_MS) {
		return { symbol: { kind: 'calm' }, heading: 0, label };
	}
	return {
		symbol: { kind: 'arrow', band: windBand(reading.speedMs) },
		heading: (reading.directionDeg + 180) % 360,
		label
	};
}

/**
 * "12/25": mean wind and gusts in whole km/h. Gusts are left out when they
 * are not known or round to the same number.
 */
export function speeds(meanMs: number, gustMs: number | undefined): string {
	const mean = wholeKilometresPerHour(meanMs);
	if (gustMs === undefined) return mean;
	const gusts = wholeKilometresPerHour(gustMs);
	return gusts === mean ? mean : `${mean}/${gusts}`;
}

/** Plain digits: these stand on the map, where no region's separators belong. */
function wholeKilometresPerHour(metresPerSecond: number): string {
	return String(Math.round(metresPerSecond * 3.6));
}

/** The name the map knows a symbol's picture by. */
export function pictureName(symbol: WindSymbol): string {
	return symbol.kind === 'arrow' ? `wind-arrow-${symbol.band}` : `wind-${symbol.kind}`;
}

/** The stations as the map's symbol layer reads them. */
export function stationFeatures(stations: readonly WindStation[]): GeoJSON.FeatureCollection {
	return {
		type: 'FeatureCollection',
		features: stations.map((station) => {
			const mark = windMark(station);
			const reading = station.latest;
			return {
				type: 'Feature',
				geometry: { type: 'Point', coordinates: [station.position.lon, station.position.lat] },
				properties: {
					id: station.id,
					picture: pictureName(mark.symbol),
					heading: mark.heading,
					scale: mark.symbol.kind === 'arrow' ? WIND_BANDS[mark.symbol.band].arrowScale : 1,
					label: mark.label,
					// Lower ranks are placed first: strong wind before weak, and
					// stations with nothing current to say last.
					rank:
						mark.symbol.kind === 'old' || !reading
							? 1000
							: -(reading.gustMs ?? reading.speedMs ?? 0)
				}
			};
		})
	};
}

/** A station's wind in a sentence, for a screen reader. */
export function spoken(station: WindStation, compass: (degrees: number) => string): string {
	const reading = station.latest;
	if (!reading || station.isOld || reading.speedMs === undefined) return 'No current reading';
	const parts = [`Wind ${wholeKilometresPerHour(reading.speedMs)} kilometres per hour`];
	if (reading.gustMs !== undefined) parts.push(`gusts ${wholeKilometresPerHour(reading.gustMs)}`);
	if (windMark(station).symbol.kind === 'arrow' && reading.directionDeg !== undefined) {
		parts.push(`from ${compass(reading.directionDeg)}`);
	}
	return parts.join(', ');
}
