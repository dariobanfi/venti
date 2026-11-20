/**
 * What the web app says about the gliders of other pilots, in a list and
 * on a glider's page. How old a position is, is put in words by the core;
 * the rest are the words of the iPhone app's `LiveScreens.swift`.
 */

import type { LiveGlider } from '#lib/core/index.ts';
import type { Format } from '#lib/design/format.ts';
import type { LiveState } from './watch';

/** What a glider on the map is worth. Said wherever gliders are listed or shown. */
export const CAUTION =
	'Positions are as each pilot’s app sent them. A glider nothing was heard of for a while has most likely no network: it is not known to have landed.';

/** Said under a list the server cut short. */
export const TRUNCATED = 'There are more than the server lists at once. Zoom the map in.';

/** Said on the page of a glider that neither the map nor the server has any more. */
export const NO_LONGER_HERE =
	'This glider is no longer on this part of the map: its pilot stopped showing the flight, or it has left the area.';

/**
 * One glider in a line of a list: how high, how it climbs, and how old the
 * position is. "1596 m · -1.0 m/s · Now". A glider on the ground does not
 * climb.
 */
export function detail(glider: LiveGlider, age: string, text: Format): string {
	const parts = [`${text.metres(glider.altitudeM)} m`];
	if (!glider.landed && glider.climbMs !== undefined) {
		parts.push(`${text.climb(glider.climbMs)} m/s`);
	}
	if (age) parts.push(age);
	return parts.join(' · ');
}

/** Why a list is empty, and what would fill it. */
export function nobodyListed(state: LiveState): { title: string; says: string } {
	return state.close
		? {
				title: 'Nobody in the air here',
				says: 'Pilots who show their flight to everyone appear on this part of the map while they fly.'
			}
		: {
				title: 'The map shows too much',
				says: 'Zoom the map in on an area, and the pilots who show their flight to everyone there are listed.'
			};
}
