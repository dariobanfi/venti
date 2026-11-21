/**
 * A made-up forecast of a strong summer's day, for the browser tests of
 * the thermal estimate: the recorded forecasts are of late November under
 * rain and cloud, which have nothing to climb in. It is NOT a forecast any
 * model made. Its air is the big day of the core's own tests
 * (`crates/core/src/today.rs`): 26 °C and a dew point of 10 °C at a model
 * ground of 812 m, the ground giving the air up to 400 W/m², convective
 * cloud at 3400 m, and the temperatures of a well-mixed layer above. It
 * has every hour of the days around the development server's clock, so
 * that whatever day that clock opens on has it.
 */

import type { Route } from '@playwright/test';

/** How the made-up day goes at a place: a big day, a quieter one, or a day of little sun. */
const DAYS = [
	{ heat: 400, base: 3400 },
	{ heat: 260, base: 2700 },
	{ heat: 120, base: 0 }
] as const;

/** The air of one hour, `local` being the hour on the valley's clock in November (UTC+1). */
function hour(time: Date, local: number, day: (typeof DAYS)[number]): object {
	// The sun's heat through the day.
	const heat =
		local >= 10 && local <= 15
			? day.heat
			: local === 9 || local === 16
				? day.heat * 0.6
				: local >= 7 && local <= 18
					? 90
					: 0;
	return {
		time: time.toISOString().replace('.000Z', 'Z'),
		wind_speed_ms: 3,
		wind_gust_ms: 5,
		wind_direction_deg: 0,
		precipitation_mm: 0,
		cape_j_kg: 150,
		...(day.base > 0 ? { convective_cloud_base_m: day.base } : {}),
		temperature_c: 26,
		dew_point_c: 10,
		heat_to_air_w_m2: heat,
		winds_aloft: [
			{ above_ground_m: 80, wind_speed_ms: 4, temperature_c: 24 },
			{ above_ground_m: 180, wind_speed_ms: 4, temperature_c: 23 },
			{ pressure_hpa: 850, altitude_m: 1500, wind_speed_ms: 4, temperature_c: 18 },
			{ pressure_hpa: 700, altitude_m: 3100, wind_speed_ms: 4, temperature_c: 3 },
			{ pressure_hpa: 600, altitude_m: 4300, wind_speed_ms: 4, temperature_c: -4 }
		]
	};
}

/**
 * Answers a question for a forecast with the made-up summer's day. Which of
 * the three kinds of day a place gets goes by where it is, so that the
 * take-offs around differ and their order shows.
 */
export async function madeUpSummer(route: Route): Promise<void> {
	const asked = new URL(route.request().url());
	const lat = Number(asked.searchParams.get('lat') ?? 47.7);
	const lon = Number(asked.searchParams.get('lon') ?? 11.8);
	const day = DAYS[Math.abs(Math.round(lon * 100)) % DAYS.length];
	const hours = [];
	for (let at = Date.UTC(2026, 10, 15); at < Date.UTC(2026, 10, 26); at += 3_600_000) {
		const time = new Date(at);
		hours.push(hour(time, (time.getUTCHours() + 1) % 24, day));
	}
	const point = { lat, lon };
	await route.fulfill({
		json: {
			point,
			fetched_at: '2026-11-15T05:40:00Z',
			layers: [
				{
					source: 'open-meteo',
					series: [
						{
							source: 'open-meteo',
							model: 'icon-d2',
							run: '2026-11-15T03:00:00Z',
							point,
							cell: point,
							cell_elevation_m: 812,
							hours
						}
					]
				}
			],
			attributions: [
				{
					source: 'open-meteo',
					provider: 'Open-Meteo',
					licence: 'CC BY 4.0',
					text: 'Weather data by Open-Meteo.com',
					url: 'https://open-meteo.com/'
				}
			]
		}
	});
}
