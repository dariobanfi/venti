/**
 * How the picture of a day's air over a take-off is laid out: heights to
 * pixels, the lines of the heights, and what stands in each hour's slot.
 * The figures are the core's (`OutlookDay`, `OutlookHour`); this only
 * places them, by the iPhone's rules (`AirChart.swift`).
 */

import type { ForecastWind, OutlookDay } from '#lib/core/index.ts';
import { thermalStep } from '#lib/design/palette.ts';

/** From a floor under the model's ground and the take-off to a ceiling over the thermals, both round. */
export interface AirScale {
	floorM: number;
	ceilingM: number;
}

/**
 * The scale for a model's ground, a take-off where its height is known,
 * how high the hours' thermals go, and the heights the model gives a wind
 * at: from the round 500 m under the lower of ground and take-off to the
 * round 500 m over the highest of the thermals, a kilometre over the
 * take-off, and the model's first level above that kilometre, with 500 m
 * of room, and at least 1500 m in all.
 */
export function airScale(
	groundM: number,
	takeoffM: number | undefined,
	tops: readonly number[],
	levels: readonly number[] = []
): AirScale {
	const low = Math.min(groundM, takeoffM ?? groundM);
	const clear = (takeoffM ?? groundM) + 1000;
	const above = Math.min(...levels.filter((level) => level >= clear));
	const high = Math.max(...tops, clear, Number.isFinite(above) ? above : 0);
	const floorM = Math.floor(low / 500) * 500;
	const ceilingM = Math.max(Math.ceil(high / 500) * 500 + 500, floorM + 1500);
	return { floorM, ceilingM };
}

/** The scale of a day at a take-off; none where the model's ground is not known. */
export function dayScale(day: OutlookDay, takeoffM: number | undefined): AirScale | undefined {
	if (day.groundM === undefined) return undefined;
	return airScale(
		day.groundM,
		takeoffM,
		day.hours.flatMap((hour) => (hour.thermalTopM === undefined ? [] : [hour.thermalTopM])),
		day.hours.flatMap((hour) => hour.winds.map((wind) => wind.altitudeM))
	);
}

/** The heights that get a line: every 500 m, or every 1000 m where the picture spans more than 3000 m. */
export function heightLines(scale: AirScale): number[] {
	const step = scale.ceilingM - scale.floorM > 3000 ? 1000 : 500;
	const lines: number[] = [];
	for (let line = Math.ceil(scale.floorM / step) * step; line <= scale.ceilingM; line += step) {
		lines.push(line);
	}
	return lines;
}

/** How far down a height lies in a plot this tall. */
export function yOf(scale: AirScale, metres: number, height: number): number {
	return height * (1 - (metres - scale.floorM) / (scale.ceilingM - scale.floorM));
}

/** What stands in one hour's slot. */
export interface AirHour {
	hour: string;
	/** The thermals' column, from the model's ground to their top, with the scale's step of the climb. */
	column: { topM: number; step: number; cloud: boolean } | undefined;
	/** The climb, where there is one to climb in. */
	climbMs: number | undefined;
	/** The winds drawn: at 10 m above the model's ground and every level more than 300 m above it, inside the picture. */
	winds: { altitudeM: number; wind: ForecastWind }[];
}

export function airHours(day: OutlookDay, scale: AirScale): AirHour[] {
	const ground = day.groundM;
	return day.hours.map((hour) => {
		const top = hour.thermalTopM;
		const column =
			top !== undefined && ground !== undefined && top > ground
				? { topM: top, step: thermalStep(hour.climbMs ?? 0), cloud: hour.thermalCloud }
				: undefined;
		const lowest = hour.winds[0]?.altitudeM;
		return {
			hour: hour.hour,
			column,
			climbMs: column && (hour.climbMs ?? 0) > 0 ? hour.climbMs : undefined,
			winds: hour.winds.filter(
				(wind) =>
					(wind.altitudeM === lowest || wind.altitudeM > (lowest ?? 0) + 300) &&
					wind.altitudeM <= scale.ceilingM
			)
		};
	});
}
