import { describe, expect, it } from 'vitest';
import type {
	ForecastDay,
	ForecastHourView,
	ForecastWind,
	ModelForecast
} from '#lib/core/index.ts';
import { compassPoint, createFormat } from '#lib/design/format.ts';
import { caveat, dayName, daysWithLight, DRY, hoursOf, rows, today, windInWords } from './table';

const text = createFormat('en-GB', 'Europe/Berlin');

/** Midnight in Munich on 20 November 2026, in seconds. */
const FRIDAY = Date.UTC(2026, 10, 19, 23) / 1000;
const HOUR = 3600;

function wind(given: Partial<ForecastWind> = {}): ForecastWind {
	return { speedMs: 3, fromDeg: 270, band: 'moderate', label: '11/20', ...given };
}

function hour(at: number, given: Partial<ForecastHourView> = {}): ForecastHourView {
	return {
		timeUnixS: at,
		isDaylight: true,
		wind: wind(),
		aloft: [wind({ label: '15' }), wind({ label: '40', band: 'strong' })],
		temperatureC: 3.4,
		cloudPct: 60,
		cloudLowPct: 40,
		cloudMidPct: 20,
		cloudHighPct: 0,
		precipitationMm: 0,
		cloudBaseM: undefined,
		freezingLevelM: 1840,
		capeJKg: 0,
		...given
	};
}

function day(start: number, hours: ForecastHourView[]): ForecastDay {
	return {
		startUnixS: start,
		sunriseUnixS: start + 7 * HOUR,
		sunsetUnixS: start + 16 * HOUR,
		hours
	};
}

function model(given: Partial<ModelForecast> = {}): ModelForecast {
	return {
		model: 'icon-d2',
		name: 'ICON-D2',
		grid: '2 km cells',
		source: 'open-meteo',
		runUnixS: FRIDAY - 5 * HOUR,
		cell: { lat: 47.68, lon: 11.9 },
		cellDistanceM: 800,
		cellElevationM: 1266,
		levels: [
			{ name: '80 m above ground', aboutAltitudeM: undefined },
			{ name: '700 hPa', aboutAltitudeM: 2900 }
		],
		days: [day(FRIDAY, [hour(FRIDAY + 9 * HOUR), hour(FRIDAY + 10 * HOUR)])],
		...given
	};
}

describe('the days and hours of a forecast worth showing', () => {
	it('are the days with the sun up in them, and of a day the hours with the sun up', () => {
		const evening = day(FRIDAY - 24 * HOUR, [hour(FRIDAY - 3 * HOUR, { isDaylight: false })]);
		const friday = day(FRIDAY, [
			hour(FRIDAY + 6 * HOUR, { isDaylight: false }),
			hour(FRIDAY + 9 * HOUR),
			hour(FRIDAY + 20 * HOUR, { isDaylight: false })
		]);

		expect(daysWithLight(model({ days: [evening, friday] }))).toEqual([friday]);
		expect(hoursOf(friday).map((hour) => text.hourOfDay(hour.timeUnixS))).toEqual(['9']);
		// A day without any daylight, far in the north in winter, shows all it has.
		expect(hoursOf(evening)).toHaveLength(1);
	});

	it('start with the day that holds this moment, or else the first', () => {
		const friday = day(FRIDAY, []);
		const saturday = day(FRIDAY + 24 * HOUR, []);

		expect(today([friday, saturday], FRIDAY + 30 * HOUR)).toBe(saturday);
		expect(today([friday, saturday], FRIDAY - HOUR)).toBe(friday);
		expect(today([], FRIDAY)).toBeUndefined();
	});

	it('are named "Today", or by weekday and day', () => {
		const friday = day(FRIDAY, []);

		expect(dayName(friday, FRIDAY + 10 * HOUR, text)).toBe('Today');
		expect(dayName(friday, FRIDAY - HOUR, text)).toBe('Fri 20');
	});
});

describe('the rows of a forecast table', () => {
	const hours = model().days[0].hours;

	it('are the wind from the highest level down to the ground, then the sky', () => {
		const table = rows(model(), hours, text);

		expect(table.map((row) => [row.name, row.detail])).toEqual([
			// A pressure surface by about how high it lies, the surface in small print.
			['2900 m', '700 hPa'],
			['80 m', 'above ground'],
			['10 m', 'with gusts'],
			['Cloud', '% of sky'],
			['Rain', 'mm'],
			['Temperature', '°C at 2 m'],
			['0 °C', 'at m']
		]);
		expect(table[0].cells[0]).toEqual({ is: 'wind', wind: wind({ label: '40', band: 'strong' }) });
		expect(table[2].cells[0]).toEqual({ is: 'wind', wind: wind() });
	});

	it('hold a square for cloud, a dot for no rain, and heights to the hundred', () => {
		const wet = [hour(FRIDAY + 9 * HOUR, { precipitationMm: 1.26, cloudBaseM: 2449 })];
		const table = rows(model(), [...hours, ...wet], text);
		const row = (name: string) => table.find((row) => row.name === name)!.cells;

		expect(row('Cloud')[0]).toEqual({ is: 'cloud', percent: 60 });
		expect(row('Rain').map((cell) => cell.is === 'number' && cell.text)).toEqual([DRY, DRY, '1.3']);
		expect(row('Temperature')[0]).toEqual({ is: 'number', text: '3' });
		expect(row('0 °C')[0]).toEqual({ is: 'number', text: '1800' });
		// An hour without convective cloud shows none; the row is there for the one that has.
		expect(row('Cloud base')).toEqual([
			{ is: 'empty' },
			{ is: 'empty' },
			{ is: 'number', text: '2400' }
		]);
	});

	it('leave out a row the model has nothing for, but never the wind at the ground', () => {
		const bare = [
			hour(FRIDAY + 9 * HOUR, {
				wind: undefined,
				aloft: [undefined, undefined],
				cloudPct: undefined,
				precipitationMm: undefined,
				temperatureC: undefined,
				freezingLevelM: undefined
			})
		];

		expect(rows(model(), bare, text)).toEqual([
			{ id: 'ground', name: '10 m', detail: 'with gusts', cells: [{ is: 'empty' }] }
		]);
	});
});

describe('what stands under a forecast table', () => {
	it('says which cell of which model, how high its ground is, and that it is no verdict', () => {
		expect(caveat(model(), text)).toBe(
			"What the weather model ICON-D2 computes for the cell of its grid that holds this place (2 km cells). The model's ground there is at 1266 m: its wind at 10 m is the wind 10 m above that, which in the mountains is not the wind at a take-off or in the valley. The cloud base is that of the model's convective cloud; an hour without one shows none. Nothing here says whether the day can be flown."
		);
	});

	it('leaves out what a model the app does not know does not say', () => {
		expect(caveat(model({ name: 'AROME', grid: '', cellElevationM: undefined }), text)).toBe(
			"What the weather model AROME computes for the cell of its grid that holds this place. The cloud base is that of the model's convective cloud; an hour without one shows none. Nothing here says whether the day can be flown."
		);
	});
});

describe('a forecast wind, read out', () => {
	it('is its speed, its gusts and where it comes from', () => {
		expect(windInWords(wind(), compassPoint)).toBe('11 gusting 20 from W');
		expect(windInWords(wind({ label: '15', fromDeg: undefined }), compassPoint)).toBe('15');
	});
});
