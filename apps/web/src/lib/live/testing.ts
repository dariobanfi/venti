/** Gliders of other pilots for unit tests, made up to the test's needs. */

import type {
	LiveAnswer,
	LiveFollowView,
	LiveGlider,
	LiveOthersAnswer,
	LiveOthersNetwork,
	OtherAircraftView
} from '#lib/core/index.ts';

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
export function answer(
	heard: number,
	gliders: LiveGlider[] = [glider()],
	others?: LiveOthersAnswer
): LiveAnswer {
	return {
		map: { gliders, truncated: false },
		heard,
		others,
		othersTooWide: false,
		tooWideWords: 'Zoom in to see the gliders of the Open Glider Network.'
	};
}

/** An aircraft the Open Glider Network hears; `given` replaces what a test cares about. */
export function aircraft(given: Partial<OtherAircraftView> = {}): OtherAircraftView {
	return {
		id: '4791c887c4a52d11',
		kind: 'paraglider',
		kindWords: 'Paraglider',
		isFreeFlight: true,
		position: { lat: 47.664083, lon: 11.7967 },
		altitudeM: 2140,
		climbMs: 1.8,
		groundSpeedMs: 7.2,
		courseDeg: 210,
		ageS: 20,
		ageWords: '20 s ago',
		name: 'WB',
		line: 'Paraglider at 2,140 m, climbing 1.8 m/s, 20 s ago',
		label: 'Paraglider · 2140 m',
		...given
	};
}

const OGN_CREDIT = {
	source: 'ogn',
	provider: 'Open Glider Network',
	text: 'Positions: Open Glider Network, glidernet.org (ODbL)',
	licence: 'ODbL 1.0',
	url: 'https://www.glidernet.org/ogn-data-usage/'
};

/** The network's answer as the worker hands it on; `heard` is its number. */
export function others(
	heard: number,
	aircraftHeard: OtherAircraftView[] = [aircraft()],
	given: Partial<LiveOthersNetwork> = {}
): LiveOthersAnswer {
	return {
		networks: [
			{
				source: 'ogn',
				connected: true,
				aircraft: aircraftHeard,
				truncated: false,
				credit: OGN_CREDIT,
				...given
			}
		],
		countWords: '1 pilot of Venti, 1 other in the air',
		credits: aircraftHeard.length > 0 ? [OGN_CREDIT] : [],
		heard
	};
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
