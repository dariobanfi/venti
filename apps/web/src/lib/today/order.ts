/**
 * The order of Today's take-offs, and the day the screen opens on: what
 * the core says of each take-off, put in order. The iPhone's
 * `TodayScreen` sorts the same way.
 */

import type { OutlookDay } from '#lib/core/index.ts';
import type { TodayPlace } from './model.svelte.ts';

/**
 * Where a take-off stands on a day, the best first: a flyable day by its
 * thermals (a big day, a cross-country day, local thermals, then weak or
 * none), then a marginal day, a day that is not flyable, and last one the
 * server has not answered for. The kind of day is the core's; this only
 * puts the kinds in order, as the iPhone's `TodayScreen` does.
 */
export function rank(day: OutlookDay | undefined): number {
	if (!day) return 6;
	if (day.verdict === 'marginal') return 4;
	if (day.verdict === 'notFlyable') return 5;
	switch (day.thermals) {
		case 'big':
			return 0;
		case 'crossCountry':
			return 1;
		case 'local':
			return 2;
		default:
			return 3;
	}
}

/**
 * The take-offs in the order the screen lists them on a day: by
 * {@link rank}, and among equals the nearer first.
 */
export function inOrder(places: readonly TodayPlace[], day: number): TodayPlace[] {
	return places.toSorted(
		(one, other) =>
			rank(one.outlook?.days[day]) - rank(other.outlook?.days[day]) ||
			one.distanceM - other.distanceM
	);
}

/**
 * The take-off that looks best on a day: the first in {@link inOrder}.
 * Nothing until one has a forecast for that day.
 */
export function bestOn(
	places: readonly TodayPlace[],
	day: number
): { place: TodayPlace; day: OutlookDay } | undefined {
	const place = inOrder(places, day)[0];
	const found = place?.outlook?.days[day];
	return found ? { place, day: found } : undefined;
}

/**
 * The day the screen opens on: the first that has flying hours left at any
 * take-off, which in the evening is tomorrow. The first day where none has
 * any.
 */
export function firstDay(places: readonly TodayPlace[]): number {
	const most = Math.max(0, ...places.map((place) => place.outlook?.days.length ?? 0));
	for (let day = 0; day < most; day += 1) {
		if (places.some((place) => (place.outlook?.days[day]?.hours.length ?? 0) > 0)) return day;
	}
	return 0;
}
