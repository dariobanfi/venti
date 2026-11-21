/**
 * The air around and above a take-off, as the screens word it: the wind
 * near by and the layers of an hour. Which wind is much stronger, where a
 * layer lies and every sentence about either are the core's
 * (`OutlookHour.nearby`, `OutlookHour.layers`); here are only the names of
 * the two figures and how an hour is read out, as on Android
 * (`AirAround.kt`).
 */

import type { OutlookHour, OutlookNearby } from '#lib/core/index.ts';

export const WIND_NEAR_BY = 'Wind near by';
export const GUSTS_NEAR_BY = 'Gusts near by';

/** The hour on the place's clock as its detail is headed: "14:00". */
export function clock(hour: Pick<OutlookHour, 'hour'>): string {
	return `${hour.hour}:00`;
}

/** One of the two figures near by: its name, and the figure with where it is. */
export interface NearbyFigure {
	name: string;
	figure: string;
}

/**
 * The strongest wind and the strongest gust near by with where they are,
 * as the core gives them and in the words of its own line: "28 km/h 20 km
 * to the south over ground at 2,400 m". The ground's height stands with
 * the figure because a square on a ridge has more wind than a valley on
 * any day. None for an hour without a neighbourhood, and none for a figure
 * the model's squares do not have.
 */
export function figures(
	nearby: OutlookNearby | undefined,
	metres: (metres: number) => string
): NearbyFigure[] {
	if (!nearby) return [];
	const figure = (name: string, kmh: string, where: string, groundM: number | undefined) => {
		if (kmh === '') return [];
		const place = where === '' ? '' : ` ${where}`;
		const ground = groundM === undefined ? '' : ` over ground at ${metres(groundM)} m`;
		return [{ name, figure: `${kmh} km/h${place}${ground}` }];
	};
	return [
		...figure(WIND_NEAR_BY, nearby.windKmh, nearby.windWhere, nearby.windGroundM),
		...figure(GUSTS_NEAR_BY, nearby.gustKmh, nearby.gustWhere, nearby.gustGroundM)
	];
}

/** Whether an hour has something to say beyond the picture: a line, a figure or a layer. */
export function hasDetail(hour: Pick<OutlookHour, 'nearby' | 'layers'>): boolean {
	return (
		hour.layers.length > 0 ||
		(hour.nearby?.line ?? '') !== '' ||
		(hour.nearby?.windKmh ?? '') !== '' ||
		(hour.nearby?.gustKmh ?? '') !== ''
	);
}

/**
 * An hour of the picture read out: what is much stronger near by, then its
 * layers, then the two figures. Empty for an hour with none of them.
 */
export function spoken(
	hour: Pick<OutlookHour, 'nearby' | 'layers'>,
	metres: (metres: number) => string
): string {
	return [
		...(hour.nearby?.line ? [hour.nearby.line] : []),
		...hour.layers.map((layer) => layer.words),
		...figures(hour.nearby, metres).map(({ name, figure }) => `${name}: ${figure}`)
	].join('. ');
}
