/**
 * The days of the flying plans.
 *
 * A plan is for a day on the calendar of the place where the flying is,
 * written `2026-11-21`, without a time zone: the server keeps it as it was
 * sent and the app shows it as it is. Which day is today is the pilot's to
 * say, and that is the date the browser's own clock shows where the pilot
 * is, not the date in UTC: at half past midnight in Munich a pilot's today
 * has begun, while UTC still has yesterday. The iPhone app's
 * `GroupsModel.swift` and `PlanWords` do the same with the phone's
 * calendar.
 */

import type { Format } from '#lib/design/format.ts';

const two = (value: number) => String(value).padStart(2, '0');

/** The day a moment falls on where the reader is, as the server writes a day. */
export function dayOf(moment: Date): string {
	return `${moment.getFullYear()}-${two(moment.getMonth() + 1)}-${two(moment.getDate())}`;
}

/** The year, month and day of a day written `2026-11-21`. Nothing for other text. */
function partsOf(day: string): [year: number, month: number, day: number] | undefined {
	const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(day);
	if (!match) return undefined;
	const [year, month, date] = match.slice(1).map(Number);
	// A date the calendar does not have, such as the 31st of November.
	const there = new Date(Date.UTC(year, month - 1, date));
	if (there.getUTCMonth() !== month - 1 || there.getUTCDate() !== date) return undefined;
	return [year, month, date];
}

/** The day after a day, both written `2026-11-21`. Nothing for other text. */
export function dayAfter(day: string): string | undefined {
	const parts = partsOf(day);
	if (!parts) return undefined;
	const next = new Date(Date.UTC(parts[0], parts[1] - 1, parts[2] + 1));
	return `${next.getUTCFullYear()}-${two(next.getUTCMonth() + 1)}-${two(next.getUTCDate())}`;
}

/**
 * What a day is called on the board: "Today", "Tomorrow", or the day with
 * its weekday, "Saturday, 21 Nov". `today` is the pilot's own today.
 * Text that is no day is shown as it is.
 */
export function dayTitle(day: string, today: string, text: Pick<Format, 'calendarDay'>): string {
	if (day === today) return 'Today';
	if (day === dayAfter(today)) return 'Tomorrow';
	const parts = partsOf(day);
	return parts ? text.calendarDay(...parts) : day;
}
