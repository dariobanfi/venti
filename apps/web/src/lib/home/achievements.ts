/**
 * The home page's big achievements: a few public flights that stand out,
 * chosen from what the server already answers (everyone's flights by
 * points, of the season and of the last days, and the newest). The points
 * and the distances are the core's reckoning; this only picks.
 */

import type { PublicFlight } from '#lib/core/index.ts';
import { isTriangle } from '#lib/community/words.ts';

/** How far back "the last days" reach. */
export const RECENT_DAYS = 14;

export interface Achievement {
	/** What the flight stands out for: "Longest flight, 2026". */
	title: string;
	flight: PublicFlight;
}

/** The scored distance, or the track's length of a flight too short to score. */
function distanceM(flight: PublicFlight): number {
	return flight.score?.distanceM ?? flight.trackLengthM;
}

/**
 * At most three flights, each once: the season's longest, its best
 * triangle, and the best of the last days, or the newest flight where
 * nobody shared one of those. `season` is the ranking's name for it,
 * "2026", or empty for all years; `seasonBest` and `recentBest` are by
 * points, the most first.
 */
export function achievements(
	season: string,
	seasonBest: readonly PublicFlight[],
	recentBest: readonly PublicFlight[],
	newest: readonly PublicFlight[]
): Achievement[] {
	const of = (title: string) => (season ? `${title}, ${season}` : title);
	const shown: Achievement[] = [];
	const add = (title: string, flight: PublicFlight | undefined) => {
		if (flight && !shown.some((one) => one.flight.id === flight.id)) shown.push({ title, flight });
	};
	const unshown = (flights: readonly PublicFlight[]) =>
		flights.find((flight) => !shown.some((one) => one.flight.id === flight.id));

	const longest = seasonBest.reduce<PublicFlight | undefined>(
		(best, flight) => (!best || distanceM(flight) > distanceM(best) ? flight : best),
		undefined
	);
	add(of('Longest flight'), longest);
	add(of('Best triangle'), unshown(seasonBest.filter(isTriangle)));
	const recent = unshown(recentBest);
	if (recent) add(`Best of the last ${RECENT_DAYS} days`, recent);
	else add('Newest flight', unshown(newest));
	return shown;
}
