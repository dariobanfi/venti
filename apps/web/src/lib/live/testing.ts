/** Gliders of other pilots for unit tests, made up to the test's needs. */

import type { LiveAnswer, LiveFollowView, LiveGlider } from '#lib/core/index.ts';

/** A glider in the air over the Tegernsee; `given` replaces what a test cares about. */
export function glider(given: Partial<LiveGlider> = {}): LiveGlider {
	return {
		id: '11111111111111111111111111',
		handle: 'demo',
		name: 'Demo Pilot',
		position: { lat: 47.6638, lon: 11.7432 },
		altitudeM: 1596,
		groundSpeedMs: 10.5,
		climbMs: -1,
		courseDeg: 20,
		landed: false,
		ageS: 4,
		quiet: false,
		label: 'Demo Pilot · 1596 m',
		pagePath: '/live/11111111111111111111111111',
		...given
	};
}

/** The server's answer for a box, as the worker hands it on; `heard` is its number. */
export function answer(heard: number, gliders: LiveGlider[] = [glider()]): LiveAnswer {
	return { map: { gliders, truncated: false }, heard };
}

/** A flight being followed; `given` replaces what a test cares about. */
export function flight(given: Partial<LiveFollowView> = {}): LiveFollowView {
	return {
		glider: glider(),
		track: [
			{ lat: 47.6636, lon: 11.7431, climbMs: -1 },
			{ lat: 47.6638, lon: 11.7432, climbMs: -1 }
		],
		startedUnixS: 1_795_027_496,
		durationS: 1200,
		highestM: 2117,
		more: false,
		...given
	};
}
