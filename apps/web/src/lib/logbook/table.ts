/**
 * The logbook as a table on a wide screen: every flight in one list, which
 * the pilot can sort by a column, narrow to a year and search. The figures
 * are the core's (`LogbookView`); this only orders and picks among them.
 */

import type { LogbookEntry, LogbookView } from '#lib/core/index.ts';

/** The columns a table can be sorted by. */
export type SortKey = 'date' | 'place' | 'airtime' | 'distance' | 'points' | 'highest' | 'climb';

export interface Sort {
	key: SortKey;
	/** Largest or latest first. */
	descending: boolean;
}

/** How the logbook opens: the newest flight first, as the core lists it. */
export const NEWEST_FIRST: Sort = { key: 'date', descending: true };

/** The scored distance where the flight was scored, its track's length where not. */
export function distanceOf(entry: LogbookEntry): number {
	return entry.score?.distanceM ?? entry.trackLengthM;
}

const VALUE: Record<SortKey, (entry: LogbookEntry) => number | string> = {
	date: (entry) => entry.startUnixS,
	place: (entry) => entry.takeoffName ?? '',
	airtime: (entry) => entry.durationS,
	distance: distanceOf,
	points: (entry) => entry.score?.points ?? 0,
	highest: (entry) => entry.maxAltitudeM,
	climb: (entry) => entry.maxClimbMs
};

/**
 * A click on a column's heading: the same column turns its order round, a
 * new one starts with the largest or latest first, a place from A.
 */
export function sortBy(sort: Sort, key: SortKey): Sort {
	if (sort.key === key) return { key, descending: !sort.descending };
	return { key, descending: key !== 'place' };
}

/** Whether a flight answers what the pilot typed: its place, glider or pilot, in any case. */
export function matches(entry: LogbookEntry, search: string): boolean {
	const words = search.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
	if (words.length === 0) return true;
	const text = [entry.takeoffName, entry.glider, entry.pilot]
		.filter(Boolean)
		.join(' ')
		.toLocaleLowerCase();
	return words.every((word) => text.includes(word));
}

/**
 * The flights the table shows: of one year or of all, those the search
 * finds, in the chosen order. Equal values keep the core's order, the
 * newest first.
 */
export function tableRows(
	view: LogbookView,
	year: number | undefined,
	search: string,
	sort: Sort
): LogbookEntry[] {
	const value = VALUE[sort.key];
	const direction = sort.descending ? -1 : 1;
	return view.years
		.filter((one) => year === undefined || one.year === year)
		.flatMap((one) => one.flights)
		.filter((entry) => matches(entry, search))
		.map((entry, index) => ({ entry, index }))
		.toSorted((one, other) => {
			const [a, b] = [value(one.entry), value(other.entry)];
			const order =
				typeof a === 'string' && typeof b === 'string'
					? a.localeCompare(b)
					: (a as number) - (b as number);
			return order * direction || one.index - other.index;
		})
		.map(({ entry }) => entry);
}
