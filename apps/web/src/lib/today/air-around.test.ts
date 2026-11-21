import { describe, expect, it } from 'vitest';
import type { OutlookLayer, OutlookNearby } from '#lib/core/index.ts';
import { clock, figures, hasDetail, spoken } from './air-around';

const metres = (value: number) => String(Math.round(value));

const NEARBY: OutlookNearby = {
	windKmh: '28',
	windWhere: '20 km to the south',
	windGroundM: 2400,
	gustKmh: '55',
	gustWhere: 'in the square around the place',
	gustGroundM: undefined,
	stronger: true,
	line: 'Gusts to 55 km/h in the square around the place, 18 km/h at the place'
};

const STABLE: OutlookLayer = {
	bottomM: 1500,
	topM: 3100,
	inversion: false,
	fallCPer100m: 0.3,
	words: 'Stable layer between 1500 and 3100 m: 0.3 °C per 100 m'
};

describe('the wind near by and the layers of an hour, in words', () => {
	it('heads an hour by its clock', () => {
		expect(clock({ hour: '14' })).toBe('14:00');
	});

	it('gives each figure with where it is and the ground there', () => {
		expect(figures(NEARBY, metres)).toEqual([
			{ name: 'Wind near by', figure: '28 km/h 20 km to the south over ground at 2400 m' },
			{ name: 'Gusts near by', figure: '55 km/h in the square around the place' }
		]);
	});

	it('has no figure the model’s squares do not have, and none without a neighbourhood', () => {
		expect(figures({ ...NEARBY, gustKmh: '', gustWhere: '' }, metres)).toHaveLength(1);
		expect(figures(undefined, metres)).toEqual([]);
	});

	it('knows an hour with nothing to say', () => {
		expect(hasDetail({ nearby: undefined, layers: [] })).toBe(false);
		expect(hasDetail({ nearby: undefined, layers: [STABLE] })).toBe(true);
		expect(hasDetail({ nearby: { ...NEARBY, stronger: false, line: '' }, layers: [] })).toBe(true);
		expect(
			hasDetail({
				nearby: { ...NEARBY, windKmh: '', gustKmh: '', stronger: false, line: '' },
				layers: []
			})
		).toBe(false);
	});

	it('reads an hour out: the line, the layers, then the figures', () => {
		expect(spoken({ nearby: NEARBY, layers: [STABLE] }, metres)).toBe(
			'Gusts to 55 km/h in the square around the place, 18 km/h at the place. ' +
				'Stable layer between 1500 and 3100 m: 0.3 °C per 100 m. ' +
				'Wind near by: 28 km/h 20 km to the south over ground at 2400 m. ' +
				'Gusts near by: 55 km/h in the square around the place'
		);
		expect(spoken({ nearby: undefined, layers: [] }, metres)).toBe('');
	});
});
