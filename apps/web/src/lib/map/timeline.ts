/**
 * The map's one line of time, for what it shows (the wind, the thermals,
 * the rain or the cloud): from what was measured in the last hours, the
 * rain radar's pictures or the wind stations of now, to the forecast's
 * hours, day after day. "Now" stands where the one meets the other.
 *
 * Every time and word in it is the core's (the radar's moments, the
 * forecast map's days and hours); this file only lays them along one line
 * and holds the few words of the app's own.
 */

import type { RadarView, ThermalMapDay, ThermalMapHoursView } from '#lib/core/index.ts';
import type { What } from './layers';

/** A stop of the line: what the map draws while the line stands there. */
export type Stop =
	/** One of the radar's moments, by its place in the radar's list. */
	| { kind: 'radar'; moment: number }
	/** The wind stations, as they measure now. */
	| { kind: 'stations' }
	/** A day of the forecast as a whole: the kind of day, square by square. */
	| { kind: 'day'; date: string }
	/** An hour of the forecast, by its place in its day's hours. */
	| { kind: 'hour'; date: string; hour: number };

export interface Cell {
	stop: Stop;
	/** What the cell shows: the clock's hour, "13", or nothing. */
	short: string;
	/** What a screen reader says of it. */
	label: string;
	/** Whether it was measured, not forecast. */
	measured: boolean;
	/** A narrow cell: a radar picture every few minutes. */
	narrow: boolean;
	/**
	 * What stands over the cell: the day's name over the first of a day of
	 * the forecast, a radar's full hour, "21:00", over its moment.
	 */
	over?: string;
	/** Whether "Now" stands over the cell. */
	now?: boolean;
	/** Whether a day of the forecast begins at the cell, after other cells: a gap before it. */
	startsDay?: boolean;
}

/**
 * What the core calls today among the forecast map's days
 * (`ThermalMapDay.name`). Read by the server's clock in the worker; the
 * development server's clock stands days before its recorded forecast,
 * whose first day is then not today.
 */
const TODAY = 'Today';

/** The app's own words, where the core has none. */
export const WORDS = {
	now: 'Now',
	measured: 'measured',
	forecast: 'forecast',
	/** The cell of a day as a whole, on the thermals' line. */
	day: 'Day'
} as const;

export interface LineOf {
	what: What;
	/** The radar's list, for the rain. */
	radar?: RadarView;
	/** Whether the wind stations have answered, for the wind. */
	stations?: boolean;
	/** The forecast map's days, the earliest first: today first. */
	days: readonly ThermalMapDay[];
	/** The hours of each of those days for the layer shown, where they were read. */
	hours?: readonly (ThermalMapHoursView | undefined)[];
}

/**
 * The cells of the line, the oldest first: what was measured, then each
 * day of the forecast, today's from the hour that has begun. The thermals
 * have the day as a whole before each day's hours.
 */
export function lineCells({ what, radar, stations, days, hours }: LineOf): Cell[] {
	const measured: Cell[] = [];
	if (what === 'rain') {
		for (const [index, moment] of (radar?.moments ?? []).entries()) {
			measured.push({
				stop: { kind: 'radar', moment: index },
				short: '',
				over: radarOver(radar!.moments, index),
				label: moment.label,
				measured: true,
				narrow: true
			});
		}
	}
	if (what === 'wind' && stations) {
		measured.push({
			stop: { kind: 'stations' },
			short: '',
			label: WORDS.now,
			measured: true,
			narrow: false,
			now: true
		});
	}
	const forecast: Cell[] = [];
	for (const [index, day] of days.entries()) {
		const view = hours?.[index];
		const ofDay: Cell[] = [];
		if (what === 'thermals') {
			ofDay.push({
				stop: { kind: 'day', date: day.date },
				short: WORDS.day,
				label: day.name,
				measured: false,
				narrow: false
			});
		}
		// Today from the hour that has begun: what came before it was measured
		// or is past. Which day is today the core says by its name for it.
		const today = index === 0 && day.name === TODAY;
		const from = today ? Math.max(view?.firstHour ?? 0, 0) : 0;
		let hoursLeft = 0;
		for (const [hour, mark] of (view?.hours ?? []).entries()) {
			if (hour < from) continue;
			hoursLeft += 1;
			ofDay.push({
				stop: { kind: 'hour', date: day.date, hour },
				short: String(mark.clockHour),
				label: `${day.name}, ${mark.label}`,
				measured: false,
				narrow: false,
				...(today && hour === from && !(what === 'wind' && stations) ? { now: true } : {})
			});
		}
		// A today whose hours are all past has nothing left to show, its day
		// as a whole neither: its name would stand over one cell beside the
		// next day's.
		if (today && view && hoursLeft === 0) continue;
		// Today's name gives way to "Now" where that stands on today's line or
		// right before it, over the wind stations: the two would crowd each
		// other and the next day's name.
		const nowBefore = today && what === 'wind' && Boolean(stations);
		if (ofDay.length > 0 && !nowBefore && !ofDay.some((cell) => cell.now)) {
			ofDay[0] = { ...ofDay[0], over: day.name };
		}
		if (ofDay.length > 0 && measured.length + forecast.length > 0) {
			ofDay[0] = { ...ofDay[0], startsDay: true };
		}
		forecast.push(...ofDay);
	}
	return [...measured, ...forecast];
}

/**
 * The time over a radar's moment: a full hour's, and the first moment's
 * where no full hour follows soon. A short run of moments has none, its
 * cells too narrow for the words, which the panel's line says.
 */
function radarOver(moments: readonly { clock: string }[], index: number): string | undefined {
	const SOON = 6;
	if (moments.length < SOON) return undefined;
	const { clock } = moments[index];
	const first = index === 0 && !moments.slice(1, SOON).some((one) => one.clock.endsWith(':00'));
	return clock.endsWith(':00') || first ? clock : undefined;
}

/** Whether two stops are the same place on the line. */
export function sameStop(one: Stop | undefined, other: Stop | undefined): boolean {
	if (!one || !other || one.kind !== other.kind) return false;
	switch (one.kind) {
		case 'radar':
			return other.kind === 'radar' && one.moment === other.moment;
		case 'stations':
			return true;
		case 'day':
			return other.kind === 'day' && one.date === other.date;
		case 'hour':
			return other.kind === 'hour' && one.date === other.date && one.hour === other.hour;
	}
}

/**
 * The words of a key's line, short: up to its first comma. "Light rain,
 * 0.2 to 1 mm an hour" is "Light rain"; the whole stands behind the "i".
 */
export function shortWords(words: string): string {
	const comma = words.indexOf(',');
	return comma === -1 ? words : words.slice(0, comma);
}

/**
 * A text of the core's without the sentences that are the project's own
 * caveat ("… is still to verify with …"), which a screen never shows
 * (owner decision 19): those stand in the documents.
 */
export function withoutCaveats(text: string): string {
	return text
		.split(/(?<=\.)\s+/)
		.filter((sentence) => !/to verify/i.test(sentence))
		.join(' ');
}
