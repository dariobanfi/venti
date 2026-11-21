/**
 * "Programs with access": what the form's date field means.
 *
 * The core names the day a token ends by the calendar of UTC ("on 21 Dec
 * 2026"), and takes the moment from which the token is no longer good. A
 * browser's date field hands over a day. So that the list names the day the
 * pilot chose, the token is good to the last second of that day in UTC.
 */

/** The moment a token ends, in seconds, from a date field's `2026-12-21`. Nothing from an empty field. */
export function endOfDayUnixS(day: string): number | undefined {
	const found = /^(\d{4})-(\d{2})-(\d{2})$/.exec(day);
	if (!found) return undefined;
	const [year, month, date] = found.slice(1).map(Number);
	const end = Date.UTC(year, month - 1, date, 23, 59, 59);
	// A day the calendar does not have, the 31st of a month of thirty, is none.
	return new Date(end).getUTCDate() === date ? end / 1000 : undefined;
}

/** The earliest day the field offers: today by UTC's calendar, whose end is still ahead. */
export function firstEndDay(nowMs: number): string {
	return new Date(nowMs).toISOString().slice(0, 10);
}
