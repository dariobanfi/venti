/**
 * The "Forecast" layer of the map tab: the kind of day, square by square.
 *
 * Everything the squares say is the core's (`thermalMap`): which day is
 * today, each square's step of the thermal scale, the key and every word
 * about a square. This file only turns a day into shapes for the map, says
 * when the server is to be asked again, and holds the few words of the
 * app's own, for when there is no map to read and for the height a square
 * was judged at. They are Android's and the iPhone's.
 */

import type { Bounds, ThermalMap, ThermalMapCell, ThermalMapDay } from '#lib/core/index.ts';
import type { Format } from '#lib/design/format.ts';

/** How long a map is shown before the server is asked again. */
export const KEEP_MS = 30 * 60_000;
/** How long after an asking that failed the layer asks again by itself. */
export const RETRY_MS = 5 * 60_000;

/** The app's own words, where the core has none. */
export const WORDS = {
	asking: 'Asking Venti for the forecast map.',
	noDays: 'The forecast map has no day from today on.',
	noneHere: 'The forecast map has no squares for this part of the map.',
	noServer: 'This copy of the app is served without Venti behind it, so there is no forecast map.'
} as const;

/** "Judged at the model's ground, 1240 m" */
export function judgedAt(groundM: number, text: Format): string {
	return `Judged at the model’s ground, ${text.metres(groundM)} m`;
}

/** "ICON-D2, run of Sat, 21 Nov at 06:00" */
export function runLine(map: Pick<ThermalMap, 'model' | 'runUnixS'>, text: Format): string {
	return `${map.model}, run of ${text.dayInYear(map.runUnixS)} at ${text.time(map.runUnixS)}`;
}

/** Whose forecast the map is, in a line for the map's lower edge; empty without credits. */
export function creditLine(map: Pick<ThermalMap, 'credits'> | undefined, text: Format): string {
	const providers = map?.credits.map((credit) => credit.provider) ?? [];
	return providers.length === 0 ? '' : `Forecast: ${text.list(providers)}`;
}

/** The day of a map with that date, or its first. */
export function dayOf(map: ThermalMap | undefined, date: string | undefined) {
	return map?.days.find((day) => day.date === date) ?? map?.days[0];
}

/** Whether a square is drawn: a kind of day, or the veil of a day that is not flyable. */
export function isDrawn(cell: ThermalMapCell): boolean {
	return cell.scaleStep !== undefined || cell.verdict === 'notFlyable';
}

/**
 * The squares of a day as the map's fill layer reads them: each with its
 * step of the thermal scale as `fill`, 0 for the veil of a day that is not
 * flyable, and its place in the day's list as `cell`. A square without an
 * estimate is left out.
 */
export function squareFeatures(day: ThermalMapDay | undefined): GeoJSON.FeatureCollection {
	return {
		type: 'FeatureCollection',
		features: (day?.cells ?? []).flatMap((cell, index) =>
			isDrawn(cell)
				? [
						{
							type: 'Feature' as const,
							properties: { cell: index, fill: cell.scaleStep ?? 0 },
							geometry: {
								type: 'Polygon' as const,
								coordinates: [
									[
										[cell.west, cell.south],
										[cell.east, cell.south],
										[cell.east, cell.north],
										[cell.west, cell.north],
										[cell.west, cell.south]
									]
								]
							}
						}
					]
				: []
		)
	};
}

/** The drawn square of a day under a place, if there is one. */
export function squareAt(
	day: ThermalMapDay | undefined,
	place: { lat: number; lon: number }
): ThermalMapCell | undefined {
	return day?.cells.find(
		(cell) =>
			isDrawn(cell) &&
			place.lat >= cell.south &&
			place.lat < cell.north &&
			place.lon >= cell.west &&
			place.lon < cell.east
	);
}

/** Whether a day has a drawn square in a part of the map. */
export function hasSquaresIn(day: ThermalMapDay, region: Bounds): boolean {
	return day.cells.some(
		(cell) =>
			isDrawn(cell) &&
			cell.south < region.north &&
			cell.north > region.south &&
			cell.west < region.east &&
			cell.east > region.west
	);
}

/**
 * Whether the server is to be asked now, at `nowMs` by this browser's
 * clock: never asked yet, or the map is older than half an hour. After an
 * asking that failed, not before five minutes have passed; "Try again"
 * asks at once without asking this.
 */
export function isDue(
	asked: { triedMs: number | undefined; fetchedMs: number | undefined },
	nowMs: number
): boolean {
	const { triedMs, fetchedMs } = asked;
	if (triedMs === undefined) return true;
	if (triedMs !== fetchedMs && nowMs - triedMs < RETRY_MS) return false;
	if (fetchedMs === undefined) return true;
	return nowMs - fetchedMs >= KEEP_MS;
}
