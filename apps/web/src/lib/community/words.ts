/**
 * What the web app says about other pilots' flights. The sentence under
 * the list is the iPhone app's, from `CommunityScreens.swift`.
 */

import type { GliderClassKind, PublicFlight, ScreenWords } from '#lib/core/index.ts';
import type { Format } from '#lib/design/format.ts';
import type { Asked, ListState } from './pages';
import { isSearching } from './flight-search';

/** Under every list: whose flights these are, and whose reckoning the numbers. */
export const RECKONING =
	'Flights their pilots share with everyone on Venti. Distances and points are Venti’s own reckoning by the World XContest’s rules, not a league’s.';

/** The same for one pilot's flights. */
export const PILOTS_RECKONING =
	'The flights this pilot shares with everyone on Venti. Distances and points are Venti’s own reckoning by the World XContest’s rules, not a league’s.';

/** When a flight was and with what: "Sat, 15 Aug 2026 · Sample Wing". */
export function detail(flight: PublicFlight, text: Format, classes?: GliderClasses): string {
	const day = text.date(flight.startUnixS);
	const glider = gliderLine(flight, classes);
	return glider ? `${day} · ${glider}` : day;
}

/** The glider classes as the core names them (`ScreenWords.gliderClasses`). */
export type GliderClasses = readonly { kind: GliderClassKind | undefined; name: string }[];

/** A class's name, the core's ("EN B"); nothing without a class or the core's names. */
export function className(
	kind: GliderClassKind | undefined,
	classes: GliderClasses | undefined
): string | undefined {
	if (!kind) return undefined;
	return classes?.find((one) => one.kind === kind)?.name;
}

/** The glider's class, then the glider as its pilot's file names it, as on the phones: "EN B · Sample Wing". */
export function gliderLine(flight: PublicFlight, classes?: GliderClasses): string | undefined {
	const known = className(flight.gliderClass, classes);
	if (flight.glider && known) return `${known} · ${flight.glider}`;
	return flight.glider ?? known;
}

/** The scored distance, or the length of the track of a flight too short to score, in kilometres. */
export function distance(flight: PublicFlight, text: Format): string {
	return text.kilometres(flight.score?.distanceM ?? flight.trackLengthM);
}

/** The core's names of the kinds of route: "Triangle". */
type RouteKinds = ScreenWords['routeKinds'] | undefined;

/** Whether a flight scored as a triangle of either kind. */
export function isTriangle(flight: PublicFlight): boolean {
	return flight.score !== undefined && flight.score.kind !== 'freeFlight';
}

/** What a triangle is called, for whoever cannot see its mark. Nothing for a free flight. */
export function shape(flight: PublicFlight, kinds: RouteKinds): string | undefined {
	return isTriangle(flight) ? kindOf(flight, kinds) : undefined;
}

/** What kind of flight it scored as: "FAI triangle". Nothing for a flight too short to score. */
export function kindOf(flight: PublicFlight, kinds: RouteKinds): string {
	return (flight.score && kinds?.[flight.score.kind]) ?? '';
}

/** A place in a row of counted flights: "1st", "2nd", "3rd", "4th". */
export function ordinal(place: number): string {
	const tens = place % 100;
	if (tens >= 11 && tens <= 13) return `${place}th`;
	return `${place}${{ 1: 'st', 2: 'nd', 3: 'rd' }[place % 10] ?? 'th'}`;
}

/** Why a ranking has no pilots, and what would change that. */
export function nothingRanked(state: {
	absent: boolean;
	problem: string | undefined;
	season: string;
	/** The month asked for, if one was, and the core's sentence for a month without a ranked flight. */
	askedMonth?: string;
	monthEmpty?: string;
}): { title: string; says: string } {
	if (state.absent) {
		return {
			title: 'Not connected to Venti',
			says: 'The ranking is made by Venti from the flights its pilots share, and this copy of the app is served without Venti behind it.'
		};
	}
	if (state.problem) {
		return { title: 'The ranking could not be fetched', says: state.problem };
	}
	if (state.askedMonth && state.monthEmpty) {
		return { title: 'Nobody is ranked', says: state.monthEmpty };
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
	asked: Pick<Asked, 'from' | 'to' | 'search'>
): { title: string; says: string } {
	if (state.absent) {
		return {
			title: 'Not connected to Venti',
			says: 'Other pilots’ flights are kept by Venti, and this copy of the app is served without Venti behind it.'
		};
	}
	if (state.problem) {
		return { title: 'The flights could not be listed', says: state.problem };
	}
	if (isSearching(asked.search)) {
		return {
			title: 'No flights match',
			says: 'Nobody shares a flight that matches these choices. Remove one, or widen the area.'
		};
	}
	if (asked.from || asked.to) {
		return {
			title: 'No public flights on these days',
			says: 'Nobody shares a flight that began on the days chosen. Choose other days, or any day.'
		};
	}
	return {
		title: 'No public flights yet',
		says: 'Flights that pilots share with everyone on Venti are listed here.'
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
