/**
 * What a pilot's page says in its head about the flights it lists: how
 * many, how far together, and the best by points. The distances and points
 * are the server's reckoning of each flight; this only adds them up.
 */

import type { PublicFlight } from '#lib/core/index.ts';

export interface Totals {
	flights: number;
	/** The scored distances together, or the track's length of a flight too short to score, metres. */
	distanceM: number;
	/** The flight with the most points, if any scores. */
	best: PublicFlight | undefined;
}

export function totalsOf(flights: readonly PublicFlight[]): Totals {
	let distanceM = 0;
	let best: PublicFlight | undefined;
	for (const flight of flights) {
		distanceM += flight.score?.distanceM ?? flight.trackLengthM;
		if (flight.score && flight.score.points > (best?.score?.points ?? -1)) best = flight;
	}
	return { flights: flights.length, distanceM, best };
}
