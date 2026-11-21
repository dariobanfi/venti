/**
 * A model's forecast for one day as a table: which days and hours are worth
 * showing, which rows there are and what stands in each cell, and the
 * sentence under it that says what the numbers are.
 *
 * The values, the levels and their names are the core's. What is laid out
 * here is the iPhone app's `ForecastScreen.swift`, with the same words.
 */

import type {
	ForecastDay,
	ForecastHourView,
	ForecastWind,
	ModelForecast
} from '#lib/core/index.ts';

/** How the screens write numbers and dates: the reader's own way. */
export interface Writing {
	metres(metres: number): string;
	temperature(celsius: number): string;
	weekdayAndDay(unixSeconds: number): string;
	hourOfDay(unixSeconds: number): string;
}

/** What one hour of one row holds. */
export type Cell =
	| { is: 'wind'; wind: ForecastWind }
	| { is: 'number'; text: string }
	| { is: 'cloud'; percent: number }
	/** The model has nothing for the hour. */
	| { is: 'empty' };

/** One row: its name, a line under the name, and what each hour holds. */
export interface Row {
	id: string;
	name: string;
	detail: string;
	cells: Cell[];
}

const EMPTY: Cell = { is: 'empty' };

/** What an hour without rain shows: a dot, so that the hours with rain stand out. */
export const DRY = '·';

/** The days worth picking: those with an hour of daylight in them. */
export function daysWithLight(model: ModelForecast): ForecastDay[] {
	return model.days.filter((day) => day.hours.some((hour) => hour.isDaylight));
}

/** The day that holds this moment, or the first one. */
export function today(days: readonly ForecastDay[], nowUnixS: number): ForecastDay | undefined {
	return (
		days.find((day) => nowUnixS >= day.startUnixS && nowUnixS < day.startUnixS + 86_400) ?? days[0]
	);
}

/**
 * "Today", or the weekday and the day of the month in short, so that a
 * forecast looked at late is not taken for another week's.
 */
export function dayName(
	day: ForecastDay,
	nowUnixS: number,
	text: Pick<Writing, 'weekdayAndDay'>
): string {
	const holdsNow = nowUnixS >= day.startUnixS && nowUnixS < day.startUnixS + 86_400;
	// Noon is on the day in question whatever the clock did at night.
	return holdsNow ? 'Today' : text.weekdayAndDay(day.startUnixS + 12 * 3600);
}

/**
 * The hours worth a column: those with the sun up. A day without any, far
 * in the north in winter, shows all it has.
 */
export function hoursOf(day: ForecastDay): ForecastHourView[] {
	const light = day.hours.filter((hour) => hour.isDaylight);
	return light.length === 0 ? day.hours : light;
}

/**
 * The rows of the table for these hours: the wind from the highest level
 * down to the ground, then cloud base, cloud, rain, temperature and the
 * height of the 0 °C level, each only if the model has it for some hour.
 */
export function rows(
	model: ModelForecast,
	hours: readonly ForecastHourView[],
	text: Pick<Writing, 'metres' | 'temperature'>
): Row[] {
	/** A height to the nearest hundred metres, which is more than a model knows of a cloud base. */
	const hundreds = (metres: number) => text.metres(Math.round(metres / 100) * 100);
	const number = (value: number | undefined, write: (value: number) => string): Cell =>
		value === undefined ? EMPTY : { is: 'number', text: write(value) };
	const some = (row: Row) => row.cells.some((cell) => cell.is !== 'empty');

	const winds: Row[] = model.levels
		.map((level, index): Row => {
			const cells = hours.map((hour): Cell => {
				const wind = hour.aloft[index];
				return wind ? { is: 'wind', wind } : EMPTY;
			});
			// A pressure surface is named by about how high it lies, with
			// the surface in small print; a height above ground by itself.
			return level.aboutAltitudeM === undefined
				? {
						id: level.name,
						name: level.name.replace(' above ground', ''),
						detail: 'above ground',
						cells
					}
				: {
						id: level.name,
						name: `${text.metres(level.aboutAltitudeM)} m`,
						detail: level.name,
						cells
					};
		})
		.filter(some)
		// The highest at the top.
		.toReversed();

	const ground: Row = {
		id: 'ground',
		name: '10 m',
		detail: 'with gusts',
		cells: hours.map((hour) => (hour.wind ? { is: 'wind', wind: hour.wind } : EMPTY))
	};

	const sky: Row[] = [
		{
			id: 'base',
			name: 'Cloud base',
			detail: 'm',
			cells: hours.map((hour) => number(hour.cloudBaseM, hundreds))
		},
		{
			id: 'cloud',
			name: 'Cloud',
			detail: '% of sky',
			cells: hours.map((hour): Cell =>
				hour.cloudPct === undefined ? EMPTY : { is: 'cloud', percent: hour.cloudPct }
			)
		},
		{
			id: 'rain',
			name: 'Rain',
			detail: 'mm',
			cells: hours.map((hour) =>
				number(hour.precipitationMm, (rain) => (rain < 0.05 ? DRY : rain.toFixed(1)))
			)
		},
		{
			id: 'temperature',
			name: 'Temperature',
			detail: '°C at 2 m',
			cells: hours.map((hour) => number(hour.temperatureC, text.temperature))
		},
		{
			id: 'freezing',
			name: '0 °C',
			detail: 'at m',
			cells: hours.map((hour) => number(hour.freezingLevelM, hundreds))
		}
	];

	// The thermals: the core's estimate from the model's heat and
	// temperatures, shown only where some hour has one.
	const thermals: Row[] = [
		{
			id: 'thermal-climb',
			name: 'Thermals, climb',
			detail: 'm/s',
			cells: hours.map((hour) => number(hour.thermalClimbMs, (climb) => climb.toFixed(1)))
		},
		{
			id: 'thermal-top',
			name: 'Thermals, top',
			detail: 'm',
			cells: hours.map((hour) =>
				number(hour.thermalTopM, (top) =>
					hour.thermalCloud ? `base ${text.metres(top)}` : text.metres(top)
				)
			)
		}
	];

	return [...winds, ground, ...sky.filter(some), ...thermals.filter(some)];
}

/** What the thermals' rows are, under the table, where it has them. */
export const THERMALS_NOTE =
	'The two rows of thermals are an estimate Venti works out from the model’s heat and temperatures, not a value of the model: the climb a paraglider can expect, and how high the thermals go, at the cloud base where it says “base”. They say nothing of whether the day can be flown.';

/** What the numbers are, and what they are not. */
export function caveat(model: ModelForecast, text: Pick<Writing, 'metres'>): string {
	let said = `What the weather model ${model.name} computes for the cell of its grid that holds this place`;
	if (model.grid !== '') said += ` (${model.grid})`;
	said += '.';
	if (model.cellElevationM !== undefined) {
		said += ` The model's ground there is at ${text.metres(model.cellElevationM)} m: its wind at 10 m is the wind 10 m above that, which in the mountains is not the wind at a take-off or in the valley.`;
	}
	return `${said} The cloud base is that of the model's convective cloud; an hour without one shows none. Nothing here says whether the day can be flown.`;
}

/** A wind in words, for a screen reader: "12 gusting 25 from W". */
export function windInWords(wind: ForecastWind, compass: (degrees: number) => string): string {
	const speed = wind.label.replace('/', ' gusting ');
	return wind.fromDeg === undefined ? speed : `${speed} from ${compass(wind.fromDeg)}`;
}
