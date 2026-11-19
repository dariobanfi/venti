/** Wind stations for unit tests, made up to the test's needs. */

import type { WindReading, WindStation } from '#lib/core/index.ts';

/** 18 November 2026, 19:05 UTC, in seconds. */
export const NOW_S = 1_795_028_700;

/** A reading taken five minutes before {@link NOW_S}; `given` replaces what a test cares about. */
export function reading(given: Partial<WindReading> = {}): WindReading {
	return {
		timeUnixS: NOW_S - 300,
		speedMs: 5,
		gustMs: 8,
		directionDeg: 270,
		temperatureC: 4,
		...given
	};
}

/** A station with a current reading; `given` replaces what a test cares about. */
export function station(given: Partial<WindStation> = {}): WindStation {
	return {
		id: 'test:1',
		name: 'Testberg',
		source: 'test',
		position: { lat: 47.5, lon: 11.5 },
		elevationM: 1200,
		latest: reading(),
		ageMin: 5,
		isOld: false,
		...given
	};
}
