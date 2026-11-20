/**
 * What the web app says about other pilots' flights. The sentence under
 * the list is the iPhone app's, from `CommunityScreens.swift`.
 */

import type { PublicFlight } from '#lib/core/index.ts';
import type { Format } from '#lib/design/format.ts';
import type { Asked, ListState } from './pages';

/** Under every list: whose flights these are, and whose reckoning the numbers. */
export const RECKONING =
	'Flights their pilots share with everyone on this site’s Venti server. Distances and points are the server’s own reckoning by the World XContest’s rules, not a league’s.';

/** The same for one pilot's flights. */
export const PILOTS_RECKONING =
	'The flights this pilot shares with everyone on this site’s Venti server. Distances and points are the server’s own reckoning by the World XContest’s rules, not a league’s.';

/** When a flight was and with what: "Sat, 15 Aug 2026 · Sample Wing". */
export function detail(flight: PublicFlight, text: Format): string {
	const day = text.day(flight.startUnixS);
	return flight.glider ? `${day} · ${flight.glider}` : day;
}

/** The scored distance, or the length of the track of a flight too short to score, in kilometres. */
export function distance(flight: PublicFlight, text: Format): string {
	return text.kilometres(flight.score?.distanceM ?? flight.trackLengthM);
}

/** What a triangle is called, for whoever cannot see its mark. Nothing for a free flight. */
export function shape(flight: PublicFlight): string | undefined {
	if (!flight.score || flight.score.kind === 'freeFlight') return undefined;
	return flight.score.kind === 'faiTriangle' ? 'FAI triangle' : 'Triangle';
}

/** Why a ranking has no pilots, and what would change that. */
export function nothingRanked(state: {
	absent: boolean;
	problem: string | undefined;
	season: string;
}): { title: string; says: string } {
	if (state.absent) {
		return {
			title: 'No Venti server here',
			says: 'A ranking is made by a Venti server from the flights its pilots share, and this site is served without one.'
		};
	}
	if (state.problem) {
		return { title: 'The ranking could not be fetched', says: state.problem };
	}
	return {
		title: 'Nobody is ranked',
		says: /^\d+$/.test(state.season)
			? `Nobody shares a flight of ${state.season} that scores. Choose another year, or all years.`
			: 'A pilot is ranked once one of their flights is shared with everyone and long enough to score.'
	};
}

/** Why a list has no flights, and what would change that. */
export function nothingListed(
	state: ListState,
	asked: Pick<Asked, 'from' | 'to'>
): { title: string; says: string } {
	if (state.absent) {
		return {
			title: 'No Venti server here',
			says: 'Other pilots’ flights are kept by a Venti server, and this site is served without one.'
		};
	}
	if (state.problem) {
		return { title: 'The flights could not be listed', says: state.problem };
	}
	if (asked.from || asked.to) {
		return {
			title: 'No public flights on these days',
			says: 'Nobody shares a flight that began on the days chosen. Choose other days, or any day.'
		};
	}
	return {
		title: 'No public flights yet',
		says: 'Flights that pilots on this site’s Venti server share with everyone are listed here.'
	};
}

/** Under the list of a place's flights: which flights these are, and whose reckoning the numbers. */
export const PLACE_RECKONING =
	'Flights that took off within a kilometre and a half of this place, as their pilots share them with everyone on Venti. Distances and points are Venti’s own reckoning by the World XContest’s rules, not a league’s.';

/**
 * Under the site on a place's page: that the site and the flights are two
 * answers. The site is its source's record, shown with its source's credit;
 * the flights are the pilots' own, asked for by where they took off.
 */
export const PLACE_APART =
	'The site is as its source has it. The flights are found by where they took off, not by the site: a flight from a take-off the source does not know is listed all the same.';

/** Why a place has no flights, and what would change that. */
export function nothingFromHere(state: ListState): { title: string; says: string } {
	if (state.absent || state.problem) return nothingListed(state, { from: '', to: '' });
	return {
		title: 'Nobody shares a flight from here',
		says: 'Flights that took off within a kilometre and a half of this place, and that their pilots share with everyone, are listed here.'
	};
}
