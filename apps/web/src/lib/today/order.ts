/**
 * Today's take-offs between the screen and the core: how each stands on a
 * day, handed to the core, and the core's order laid over the take-offs.
 * Which take-off comes first and which is the best is the core's
 * (`today_order`); nothing here ranks. The day the screen opens on is
 * still decided here ({@link firstDay}), as on the iPhone's `TodayScreen`.
 */

import type { OutlookDay, TodayOrder, TodayStanding } from '#lib/core/index.ts';
import type { TodayPlace } from './model.svelte.ts';

/**
 * How every take-off stands on each day the screen can show, for the core
 * to put in order: one list for a day, of as many days as the longest
 * forecast has, and of one while none has arrived.
 */
export function standings(places: readonly TodayPlace[]): TodayStanding[][] {
	const days = Math.max(1, ...places.map((place) => place.outlook?.days.length ?? 0));
	return Array.from({ length: days }, (_, day) =>
		places.map((place) => {
			const found = place.outlook?.days[day];
			return {
				id: place.site.id,
				isKept: place.isKept,
				distanceM: place.distanceM,
				verdict: found?.verdict,
				thermals: found?.thermals
			};
		})
	);
}

/** Today's take-offs as the core ordered them on a day. */
export interface Arranged {
	/** Every take-off, in the core's order. */
	listed: TodayPlace[];
	/** The take-off the core calls the best of the day, once it has a forecast for it. */
	best: { place: TodayPlace; day: OutlookDay } | undefined;
}

/**
 * The take-offs in the order the core gave for a day, and its best. Until
 * the core has answered they stand as it listed them. A take-off the order
 * does not name, because it was asked for a moment before the take-off
 * came, stands after the others.
 */
export function arranged(
	places: readonly TodayPlace[],
	order: TodayOrder | undefined,
	day: number
): Arranged {
	if (!order) return { listed: [...places], best: undefined };
	const byId = new Map(places.map((place) => [place.site.id, place]));
	const named = order.ids.flatMap((id) => byId.get(id) ?? []);
	const listed = [...named, ...places.filter((place) => !named.includes(place))];
	const place = order.best === undefined ? undefined : byId.get(order.best);
	const found = place?.outlook?.days[day];
	return { listed, best: place && found ? { place, day: found } : undefined };
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
