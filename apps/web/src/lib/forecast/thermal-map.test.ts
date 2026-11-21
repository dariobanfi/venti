import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import type { ThermalMap, ThermalMapCell, ThermalMapDay } from '#lib/core/index.ts';
import { format } from '#lib/design/format.ts';
import { keyColor, NOT_FLYABLE_ON_MAP, THERMAL_ON_MAP } from '#lib/design/palette.ts';
import style from '../../../../../map/style.json';
import { hourFill, layerAbove, thermalLayer } from './thermal-layer';
import { ThermalMapModel } from './thermal-map.svelte';
import {
	creditLine,
	dayOf,
	hasSquaresIn,
	hoursAsked,
	isDue,
	judgedAt,
	KEEP_MS,
	RETRY_MS,
	runLine,
	sameAsked,
	shownHour,
	squareAt,
	squareFeatures
} from './thermal-map';

const text = format();

function cell(south: number, west: number, more: Partial<ThermalMapCell> = {}): ThermalMapCell {
	return {
		south,
		west,
		north: south + 0.1,
		east: west + 0.1,
		verdict: 'flyable',
		thermals: 'crossCountry',
		scaleStep: 5,
		status: 'Flyable',
		why: '',
		thermalWords: 'Cross-country day',
		thermalLine: '1.8 m/s to a cloud base of 3250 m',
		groundM: 1240,
		...more
	};
}

/** A cross-country square, one that is not flyable, and one without an estimate. */
const DAY: ThermalMapDay = {
	date: '2026-07-12',
	name: 'Today',
	whole: true,
	note: '',
	model: 'ICON-D2',
	runUnixS: 1_783_825_200,
	cells: [
		cell(47.5, 11.5),
		cell(47.5, 11.6, {
			verdict: 'notFlyable',
			thermals: undefined,
			scaleStep: undefined,
			status: 'Not flyable',
			why: 'Rain',
			thermalWords: '',
			thermalLine: ''
		}),
		cell(47.5, 11.7, {
			verdict: 'marginal',
			thermals: undefined,
			scaleStep: undefined,
			thermalWords: '',
			thermalLine: ''
		})
	]
};

describe('the squares of a day', () => {
	it('are drawn in their step, the day that is not flyable as the veil, and none without an estimate', () => {
		const drawn = squareFeatures(DAY).features;
		expect(drawn.map((feature) => feature.properties)).toEqual([
			{ cell: 0, fill: 5 },
			{ cell: 1, fill: 0 }
		]);
		expect(drawn[0].geometry).toEqual({
			type: 'Polygon',
			coordinates: [
				[
					[11.5, 47.5],
					[11.6, 47.5],
					[11.6, 47.6],
					[11.5, 47.6],
					[11.5, 47.5]
				]
			]
		});
		expect(squareFeatures(undefined).features).toEqual([]);
	});

	it('are found under a place, but not where nothing is drawn', () => {
		expect(squareAt(DAY, { lat: 47.55, lon: 11.55 })).toBe(DAY.cells[0]);
		expect(squareAt(DAY, { lat: 47.55, lon: 11.65 })).toBe(DAY.cells[1]);
		expect(squareAt(DAY, { lat: 47.55, lon: 11.75 })).toBeUndefined();
		expect(squareAt(DAY, { lat: 48, lon: 11.55 })).toBeUndefined();
	});

	it('say whether a part of the map has any', () => {
		expect(hasSquaresIn(DAY, { south: 47.4, west: 11.4, north: 47.52, east: 11.52 })).toBe(true);
		expect(hasSquaresIn(DAY, { south: 47.4, west: 11.71, north: 47.52, east: 11.9 })).toBe(false);
		expect(hasSquaresIn(DAY, { south: 48, west: 11, north: 48.5, east: 12 })).toBe(false);
	});
});

describe('the map', () => {
	const map: ThermalMap = {
		days: [DAY, { ...DAY, date: '2026-07-13', name: 'Tomorrow' }],
		legend: [],
		layers: [],
		levels: [],
		model: 'ICON-D2',
		runUnixS: Date.UTC(2026, 6, 12, 4) / 1000,
		computedUnixS: 0,
		note: '',
		credits: [
			{
				source: 'open-meteo',
				provider: 'Open-Meteo',
				text: '',
				licence: 'CC BY 4.0',
				url: ''
			}
		]
	};

	it('shows the day chosen, or its first', () => {
		expect(dayOf(map, '2026-07-13')?.name).toBe('Tomorrow');
		expect(dayOf(map, undefined)?.name).toBe('Today');
		expect(dayOf(map, '2026-07-20')?.name).toBe('Today');
		expect(dayOf(undefined, undefined)).toBeUndefined();
	});

	it('says its model, its run and whose it is', () => {
		// The day and the time are written as the reader's region writes them.
		expect(runLine(DAY, text)).toBe(
			`ICON-D2, run of ${text.dayInYear(DAY.runUnixS)} at ${text.time(DAY.runUnixS)}`
		);
		expect(runLine(DAY, text)).toMatch(/^ICON-D2, run of Sun, /);
		// A later day can be a coarser model's, with a run of its own.
		expect(runLine({ model: 'ICON-EU', runUnixS: DAY.runUnixS - 10_800 }, text)).toMatch(
			/^ICON-EU, run of /
		);
		expect(creditLine(map, text)).toBe('Forecast: Open-Meteo');
		expect(creditLine(undefined, text)).toBe('');
		expect(judgedAt(1240, text)).toBe('Judged at the model’s ground, 1240 m');
	});
});

describe('asking for the map', () => {
	const NOW = 1_000_000_000;

	it('is due at once, and again when the map is half an hour old', () => {
		expect(isDue({ triedMs: undefined, fetchedMs: undefined }, NOW)).toBe(true);
		expect(isDue({ triedMs: NOW, fetchedMs: NOW }, NOW + KEEP_MS - 1)).toBe(false);
		expect(isDue({ triedMs: NOW, fetchedMs: NOW }, NOW + KEEP_MS)).toBe(true);
	});

	it('waits five minutes after an asking that failed', () => {
		expect(isDue({ triedMs: NOW, fetchedMs: undefined }, NOW + RETRY_MS - 1)).toBe(false);
		expect(isDue({ triedMs: NOW, fetchedMs: undefined }, NOW + RETRY_MS)).toBe(true);
		// A map fetched before stays until it is half an hour old.
		const earlier = NOW - KEEP_MS + 10 * 60_000;
		expect(isDue({ triedMs: NOW, fetchedMs: earlier }, NOW + RETRY_MS)).toBe(false);
		expect(isDue({ triedMs: NOW, fetchedMs: earlier }, NOW + 10 * 60_000)).toBe(true);
	});
});

describe('the layer of squares', () => {
	it('takes the light theme’s thermal steps and the veil, see-through by the zoom', () => {
		const layer = thermalLayer();
		expect(layer.paint?.['fill-color']).toEqual([
			'match',
			['get', 'fill'],
			...THERMAL_ON_MAP.flatMap((color, index) => [index + 1, color]),
			NOT_FLYABLE_ON_MAP
		]);
		expect(layer.paint?.['fill-opacity']).toEqual([
			'interpolate',
			['linear'],
			['zoom'],
			8,
			0.5,
			12,
			0.3
		]);
	});

	it('lies under the base map’s cable cars, borders and names', () => {
		const layers = [
			{ id: 'land', type: 'fill' },
			{ id: 'road-major', type: 'line' },
			{ id: 'aerial-aerialway-casing', type: 'line' },
			{ id: 'aerialway', type: 'line' },
			{ id: 'boundary', type: 'line' },
			{ id: 'place', type: 'symbol' }
		];
		expect(layerAbove(layers)).toBe('aerial-aerialway-casing');
		expect(layerAbove(layers.slice(4))).toBe('boundary');
		expect(layerAbove(layers.slice(5))).toBe('place');
		expect(layerAbove(layers.slice(0, 2))).toBeUndefined();
		// The base map's own style has them.
		expect(layerAbove(style.layers)).toBe('aerial-aerialway-casing');
	});
});

describe('the hours of a day, layer by layer', () => {
	const choice = (layer: string, hasLevels = false) => ({
		layer: layer as never,
		name: layer,
		hasLevels,
		hasArrows: hasLevels,
		legend: [],
		note: ''
	});
	const withHours = {
		layers: [choice('thermals'), choice('wind', true), choice('gusts'), choice('cloud')],
		levels: [
			{ heightM: undefined, name: 'Ground', words: '' },
			{ heightM: 2000, name: '2000 m', words: '' }
		]
	};
	const day = { date: '2026-07-12' };

	it('are asked for by the day, the layer and, for the wind, the height', () => {
		expect(hoursAsked(withHours, day, 'cloud', 2000)).toEqual({
			date: '2026-07-12',
			layer: 'cloud'
		});
		expect(hoursAsked(withHours, day, 'wind', 2000)).toEqual({
			date: '2026-07-12',
			layer: 'wind',
			heightM: 2000
		});
		// A height the map does not have, or none: the ground.
		expect(hoursAsked(withHours, day, 'wind', 1234)).toEqual({ date: '2026-07-12', layer: 'wind' });
		// A layer the map does not offer is read as its first.
		expect(hoursAsked(withHours, day, 'rain', undefined)?.layer).toBe('thermals');
	});

	it('are not asked for where the map has none: the layer is the days alone', () => {
		expect(hoursAsked({ layers: [], levels: [] }, day, 'wind', undefined)).toBeUndefined();
		expect(hoursAsked(undefined, day, 'wind', undefined)).toBeUndefined();
		expect(sameAsked(undefined, undefined)).toBe(true);
		expect(
			sameAsked({ date: 'a', layer: 'wind', heightM: 1000 }, { date: 'a', layer: 'wind' })
		).toBe(false);
	});

	it('open on the day as a whole on "Thermals", else at the hour the core names', () => {
		const hours = [0, 1, 2, 3].map((index) => ({
			timeUnixS: index,
			clockHour: 10 + index,
			label: `${10 + index}:00`
		}));
		expect(shownHour({ layer: 'thermals', hours, firstHour: 2 }, undefined)).toBeUndefined();
		expect(shownHour({ layer: 'wind', hours, firstHour: 2 }, undefined)).toBe(2);
		expect(shownHour({ layer: 'wind', hours, firstHour: 2 }, 1)).toBe(1);
		// "Day" is no choice on the other layers, and an hour the day lacks is none.
		expect(shownHour({ layer: 'wind', hours, firstHour: 2 }, 'day')).toBe(2);
		expect(shownHour({ layer: 'thermals', hours, firstHour: 2 }, 9)).toBeUndefined();
	});

	it('colour a square by the line of the key that has its step', () => {
		const fill = hourFill([
			{ step: 1, words: 'Light', scale: 'wind', scaleStep: 2 },
			{ step: 2, words: 'Overcast', scale: 'veil', scaleStep: 0 }
		]) as unknown[];
		expect(fill.slice(0, 2)).toEqual(['match', ['get', 'step']]);
		expect(fill[3]).toBe(keyColor({ scale: 'wind', scaleStep: 2 }));
		expect(fill[5]).toBe(NOT_FLYABLE_ON_MAP);
		expect(keyColor({ scale: 'thermal', scaleStep: 3 })).toBe(THERMAL_ON_MAP[2]);
		expect(keyColor({ scale: 'vario', scaleStep: 4 })).toMatch(/^#/);
	});
});

describe('the card’s key and notes, which a narrow screen folds away', () => {
	/** A map of two days, and a core that has it and nothing else to say. */
	const MAP: ThermalMap = {
		days: [DAY, { ...DAY, date: '2026-07-13', name: 'Tomorrow' }],
		legend: [],
		layers: [],
		levels: [],
		model: 'ICON-D2',
		runUnixS: 0,
		computedUnixS: 0,
		note: '',
		credits: []
	};
	const core = { thermalMap: async () => MAP } as unknown as ConstructorParameters<
		typeof ThermalMapModel
	>[0];

	// The model reads the layer the pilot left from the browser's storage,
	// of which this process has none.
	beforeAll(() => vi.stubGlobal('localStorage', undefined));
	afterAll(() => vi.unstubAllGlobals());

	it('are folded when the app opens', () => {
		expect(new ThermalMapModel(core).keyOpen).toBe(false);
	});

	it('stay as the pilot left them over another day and another map', async () => {
		const model = new ThermalMapModel(core);
		await model.load();
		model.keyOpen = true;
		model.chooseDay(MAP.days[1].date);
		expect(model.day?.date).toBe(MAP.days[1].date);
		await model.load();
		expect(model.keyOpen).toBe(true);
		// Folded again, they stay folded.
		model.keyOpen = false;
		model.chooseDay(MAP.days[0].date);
		expect(model.keyOpen).toBe(false);
	});

	it('are folded again in another visit: the browser keeps nothing of it', () => {
		const model = new ThermalMapModel(core);
		model.keyOpen = true;
		expect(new ThermalMapModel(core).keyOpen).toBe(false);
	});
});
