/** Flying sites for unit tests, made up to the test's needs. */

import type { Credit, SitesMap, SiteView } from '#lib/core/index.ts';

/** A take-off the source says little about; `given` replaces what a test cares about. */
export function site(given: Partial<SiteView> = {}): SiteView {
	return {
		id: 'test:node/1',
		name: 'Take-off',
		isNamed: false,
		kind: 'takeoff',
		uses: ['takeoff'],
		kindInWords: 'Take-off',
		position: { lat: 47.5, lon: 11.5 },
		elevationM: undefined,
		wind: [],
		windInWords: '',
		official: undefined,
		discouraged: false,
		paragliding: undefined,
		hangGliding: undefined,
		source: 'test',
		url: undefined,
		website: undefined,
		description: undefined,
		fee: undefined,
		feeInWords: '',
		guestGuidelines: undefined,
		guestsInWords: '',
		noFlightTime: undefined,
		noFlightInWords: '',
		outline: [],
		...given
	};
}

export const CREDIT: Credit = {
	source: 'test',
	provider: 'Testkarte',
	notice: 'Anyone can correct it: https://example.org/fix',
	text: '© Testkarte',
	licence: 'ODbL 1.0',
	url: 'https://example.org/copyright'
};

/** The server's answer for a box, as the core reads it; `given` replaces what a test cares about. */
export function answer(given: Partial<SitesMap> = {}): SitesMap {
	return { sites: [site()], credits: [CREDIT], asOfUnixS: 1_795_028_700, isWhole: true, ...given };
}
