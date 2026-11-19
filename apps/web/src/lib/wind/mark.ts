/**
 * How one wind station is put on the map and read out in a list.
 *
 * What a station is drawn as is the core's decision and arrives with the
 * station (`station.mark`): an arrow in one of four bands of wind, a dot
 * for calm, a dot for a reading too old to stand for the wind now, where
 * the arrow points and the numbers under it. This file only turns that
 * into what the map engine and a screen reader take.
 */

import type { WindStation, WindSymbol } from '#lib/core/index.ts';
import { WIND_BANDS } from '#lib/design/palette.ts';

/** The name the map knows a symbol's picture by. */
export function pictureName(symbol: WindSymbol): string {
	return symbol.kind === 'arrow' ? `wind-arrow-${symbol.band}` : `wind-${symbol.kind}`;
}

/** The stations as the map's symbol layer reads them. */
export function stationFeatures(stations: readonly WindStation[]): GeoJSON.FeatureCollection {
	return {
		type: 'FeatureCollection',
		features: stations.map((station) => {
			const { symbol, headingDeg, label } = station.mark;
			const reading = station.latest;
			return {
				type: 'Feature',
				geometry: { type: 'Point', coordinates: [station.position.lon, station.position.lat] },
				properties: {
					id: station.id,
					picture: pictureName(symbol),
					heading: headingDeg,
					scale: symbol.kind === 'arrow' ? WIND_BANDS[symbol.band].arrowScale : 1,
					label,
					// Where symbols crowd, lower ranks are placed first: strong
					// wind before weak, and stations with nothing current to say
					// last.
					rank: symbol.kind === 'old' ? 1000 : -(reading?.gustMs ?? reading?.speedMs ?? 0)
				}
			};
		})
	};
}

/** A station's wind in a sentence, for a screen reader. */
export function spoken(station: WindStation, compass: (degrees: number) => string): string {
	const reading = station.latest;
	const { symbol } = station.mark;
	if (symbol.kind === 'old' || reading?.speedMs === undefined) return 'No current reading';
	const mean = wholeKilometresPerHour(reading.speedMs);
	const parts = [`Wind ${mean} ${mean === '1' ? 'kilometre' : 'kilometres'} per hour`];
	if (reading.gustMs !== undefined) parts.push(`gusts ${wholeKilometresPerHour(reading.gustMs)}`);
	if (symbol.kind === 'arrow' && reading.directionDeg !== undefined) {
		parts.push(`from ${compass(reading.directionDeg)}`);
	}
	return parts.join(', ');
}

function wholeKilometresPerHour(metresPerSecond: number): string {
	return String(Math.round(metresPerSecond * 3.6));
}
