import { describe, expect, it } from 'vitest';
import type { RouteAlong, RouteWeatherAsked } from '#lib/core/index.ts';
import {
	dateOf,
	DEFAULT_GLIDE,
	DEFAULT_SPEED_KMH,
	keptSetting,
	nextFullHour,
	RouteWeatherModel,
	startUnixS
} from './weather.svelte';

function memory() {
	const kept = new Map<string, string>();
	return {
		getItem: (key: string) => kept.get(key) ?? null,
		setItem: (key: string, value: string) => void kept.set(key, value)
	};
}

const settled = () => new Promise((resolve) => setTimeout(resolve, 0));
const along = (words: string) => ({ words }) as RouteAlong;
const A = { lat: 47.6642, lon: 11.7967, name: 'Wallberg' };
const B = { lat: 47.62, lon: 11.91 };

describe('the start, the speed and the glide of a planned route', () => {
	it('starts at the next full hour of today, on the browser’s clock', () => {
		const now = new Date(2026, 10, 22, 13, 25);
		expect(nextFullHour(now)).toBe('14:00');
		expect(dateOf(now)).toBe('2026-11-22');
		expect(startUnixS('2026-11-22', '14:00')).toBe(new Date(2026, 10, 22, 14).getTime() / 1000);
	});

	it('keeps the speed and the glide, 20 km/h and 8 until the pilot says otherwise', () => {
		const storage = memory();
		expect(keptSetting(storage)).toEqual({ speedKmh: DEFAULT_SPEED_KMH, glide: DEFAULT_GLIDE });
		const model = new RouteWeatherModel(storage, async () => undefined);
		model.speedKmh = 25;
		model.glide = 9.5;
		model.keep();
		expect(keptSetting(storage)).toEqual({ speedKmh: 25, glide: 9.5 });
		expect(new RouteWeatherModel(storage, async () => undefined).glide).toBe(9.5);
	});

	it('asks for the chosen day of the thermal map, and drops an answer that is out of date', async () => {
		const asked: RouteWeatherAsked[] = [];
		const waiting: (() => void)[] = [];
		const model = new RouteWeatherModel(
			memory(),
			(question) =>
				new Promise((resolve) => {
					asked.push(question);
					waiting.push(() => resolve({ along: along(`${question.setting.groundSpeedKmh}`) }));
				}),
			() => new Date(2026, 10, 22, 9, 10)
		);
		const days = [{ date: '2026-11-22' }, { date: '2026-11-23' }];
		void model.show([A], days);
		expect(asked).toEqual([]);

		model.date = '2026-11-23';
		void model.show([A, B], days);
		model.speedKmh = 30;
		void model.show([A, B], days);
		expect(asked.map((one) => one.date)).toEqual(['2026-11-23', '2026-11-23']);
		expect(asked[0].setting.startUnixS).toBe(new Date(2026, 10, 23, 10).getTime() / 1000);
		expect(asked[0].setting.places[0].name).toBe('Wallberg');
		// The newer answer first, the older after it: the newer stands.
		waiting[1]();
		await settled();
		waiting[0]();
		await settled();
		expect(model.along?.words).toBe('30');
	});
});
