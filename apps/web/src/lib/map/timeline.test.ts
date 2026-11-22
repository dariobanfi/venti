import { describe, expect, it } from 'vitest';
import type { RadarView, ThermalMapDay, ThermalMapHoursView } from '#lib/core/index.ts';
import { lineCells, sameStop, shortWords, withoutCaveats } from './timeline';

const day = (date: string, name: string) => ({ date, name }) as ThermalMapDay;
const hours = (firstHour: number, ...clock: number[]) =>
	({
		firstHour,
		hours: clock.map((hour) => ({
			timeUnixS: hour * 3600,
			clockHour: hour,
			label: `${hour}:00`
		}))
	}) as unknown as ThermalMapHoursView;
const radar = (...clocks: string[]) =>
	({
		moments: clocks.map((clock) => ({ clock, label: `Rain radar, ${clock}` }))
	}) as unknown as RadarView;

const DAYS = [day('2026-11-22', 'Today'), day('2026-11-23', 'Tomorrow')];

describe('the map’s line of time', () => {
	it('runs from the radar’s moments to the forecast’s hours, today’s from the hour begun, with Now between', () => {
		const cells = lineCells({
			what: 'rain',
			radar: radar('20:35', '20:40', '20:45', '20:50', '20:55', '21:00', '21:05'),
			days: DAYS,
			hours: [hours(1, 20, 21, 22), hours(0, 9, 10)]
		});
		expect(cells.map((cell) => cell.short).slice(7)).toEqual(['21', '22', '9', '10']);
		expect(cells.filter((cell) => cell.measured)).toHaveLength(7);
		expect(cells.findIndex((cell) => cell.now)).toBe(7);
		expect(cells.map((cell) => cell.over ?? '')).toEqual([
			...['', '', '', '', '', '21:00', ''],
			...['', '', 'Tomorrow', '']
		]);
		expect(cells[7].stop).toEqual({ kind: 'hour', date: '2026-11-22', hour: 1 });
		// A short run of moments has no time over it: its cells are too narrow.
		const short = lineCells({ what: 'rain', radar: radar('14:10', '14:15'), days: [] });
		expect(short.map((cell) => cell.over)).toEqual([undefined, undefined]);
	});

	it('has the wind stations as Now, and the thermals’ day as a whole before its hours', () => {
		const wind = lineCells({ what: 'wind', stations: true, days: DAYS, hours: [hours(0, 13)] });
		expect(wind[0]).toMatchObject({ stop: { kind: 'stations' }, now: true, measured: true });
		expect(wind.filter((cell) => cell.now)).toHaveLength(1);

		const thermals = lineCells({ what: 'thermals', days: DAYS });
		expect(thermals.map((cell) => cell.stop)).toEqual([
			{ kind: 'day', date: '2026-11-22' },
			{ kind: 'day', date: '2026-11-23' }
		]);
	});

	it('cuts no hours of a first day that is not today', () => {
		const cells = lineCells({
			what: 'cloud',
			days: [day('2026-11-21', 'Sat')],
			hours: [hours(4, 9, 10, 11, 12, 13)]
		});
		expect(cells).toHaveLength(5);
		expect(cells.some((cell) => cell.now)).toBe(false);
	});

	it('tells stops apart', () => {
		expect(sameStop({ kind: 'stations' }, { kind: 'stations' })).toBe(true);
		expect(sameStop({ kind: 'radar', moment: 1 }, { kind: 'radar', moment: 2 })).toBe(false);
		expect(
			sameStop({ kind: 'hour', date: 'a', hour: 1 }, { kind: 'hour', date: 'a', hour: 1 })
		).toBe(true);
		expect(sameStop({ kind: 'day', date: 'a' }, undefined)).toBe(false);
	});
});

describe('the words of the panel', () => {
	it('show a key’s line up to its comma, and no caveat of the project’s own', () => {
		expect(shortWords('Light rain, 0.2 to 1 mm an hour')).toBe('Light rain');
		expect(shortWords('10 to 20 km/h')).toBe('10 to 20 km/h');
		expect(
			withoutCaveats('It is no forecast. That this is so is still to verify with them. Last.')
		).toBe('It is no forecast. Last.');
	});
});
