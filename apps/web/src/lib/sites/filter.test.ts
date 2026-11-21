import { describe, expect, it } from 'vitest';
import type { SitesFilter, SitesFiltered } from '#lib/core/index.ts';
import { reading, station } from '#lib/wind/testing.ts';
import {
	asked,
	keepChoice,
	keptChoice,
	measuredNow,
	NO_CHOICE,
	sameAsked,
	shownNow,
	type SitesChoice
} from './filter';
import { site } from './testing';

function memory() {
	const kept = new Map<string, string>();
	return {
		kept,
		getItem: (key: string) => kept.get(key) ?? null,
		setItem: (key: string, value: string) => void kept.set(key, value)
	};
}

const NORTH_WEST: SitesChoice = {
	shown: 'takeoffs',
	windIs: 'chosen',
	windFromDeg: 315,
	takeoffsFromM: 1500
};

describe('the filter of the sites as the pilot left it', () => {
	it('asks nothing until the pilot chooses, and is remembered by the browser', () => {
		const storage = memory();
		expect(keptChoice(storage)).toEqual(NO_CHOICE);

		keepChoice(storage, NORTH_WEST);
		expect(keptChoice(storage)).toEqual(NORTH_WEST);
	});

	it('keeps no direction for the wind now, which is read anew wherever the map is', () => {
		const storage = memory();
		keepChoice(storage, { ...NO_CHOICE, windIs: 'measuredNow', windFromDeg: 200 });

		expect(keptChoice(storage)).toEqual({ ...NO_CHOICE, windIs: 'measuredNow' });
	});

	it('asks nothing where nothing can be read, or what is there is no filter', () => {
		expect(keptChoice(undefined)).toEqual(NO_CHOICE);
		expect(
			keptChoice({
				getItem: () => {
					throw new Error('no storage');
				}
			})
		).toEqual(NO_CHOICE);
		const storage = memory();
		storage.kept.set('venti.map-sites-filter', 'not json');
		expect(keptChoice(storage)).toEqual(NO_CHOICE);
		storage.kept.set(
			'venti.map-sites-filter',
			JSON.stringify({
				shown: 'winches',
				windIs: 'guessed',
				windFromDeg: 'NW',
				takeoffsFromM: null
			})
		);
		expect(keptChoice(storage)).toEqual(NO_CHOICE);
		// Storage that refuses to keep anything changes nothing else.
		keepChoice(
			{
				setItem: () => {
					throw new Error('full');
				}
			},
			NORTH_WEST
		);
	});
});

describe('what the core is asked', () => {
	it('is the pilot’s choice, with the direction a station measures where that is chosen', () => {
		expect(asked(NORTH_WEST, { measuredDeg: 200 })).toEqual({
			shown: 'takeoffs',
			windIs: 'chosen',
			windFromDeg: 315,
			takeoffsFromM: 1500
		});
		const now: SitesChoice = { ...NO_CHOICE, windIs: 'measuredNow' };
		expect(asked(now, { measuredDeg: 200 })).toMatchObject({
			windIs: 'measuredNow',
			windFromDeg: 200
		});
		// No station at hand, and no forecast at all: no wind is asked about.
		expect(asked(now, { measuredDeg: undefined }).windFromDeg).toBeUndefined();
		expect(
			asked({ ...NO_CHOICE, windIs: 'forecastNow' }, { measuredDeg: 200 }).windFromDeg
		).toBeUndefined();
	});

	it('is the same when every part of it is', () => {
		const filter = asked(NORTH_WEST, { measuredDeg: undefined });
		expect(sameAsked(filter, { ...filter })).toBe(true);
		expect(sameAsked(filter, { ...filter, windFromDeg: 292.5 })).toBe(false);
		expect(sameAsked(filter, { ...filter, takeoffsFromM: undefined })).toBe(false);
	});
});

describe('the wind measured now', () => {
	const west = station({ id: 'west', position: { lat: 47.7, lon: 11.7 } });
	const south = station({
		id: 'south',
		position: { lat: 47.6, lon: 11.9 },
		latest: reading({ directionDeg: 180 })
	});

	it('is the direction of the station nearest to the middle of the map', () => {
		expect(measuredNow([west, south], { lat: 47.7, lon: 11.72 })).toBe(270);
		expect(measuredNow([west, south], { lat: 47.61, lon: 11.9 })).toBe(180);
	});

	it('is not taken from a station that is old or names no direction', () => {
		const old = { ...west, isOld: true };
		const turning = { ...west, latest: reading({ directionDeg: undefined }) };
		const silent = { ...west, latest: undefined };

		expect(measuredNow([old, turning, silent, south], { lat: 47.7, lon: 11.7 })).toBe(180);
		expect(measuredNow([old, turning, silent], { lat: 47.7, lon: 11.7 })).toBeUndefined();
	});

	it('is none without a station, or before the map has said where it is', () => {
		expect(measuredNow([], { lat: 47.7, lon: 11.7 })).toBeUndefined();
		expect(measuredNow([west], undefined)).toBeUndefined();
	});
});

describe('what is shown of the sites', () => {
	const ramp = site({ id: 'test:node/1' });
	const slope = site({ id: 'test:node/2' });
	const field = site({ id: 'test:node/3', kind: 'landing', uses: ['landing'] });
	const sites = [ramp, slope, field];
	const filter: SitesFilter = asked(NORTH_WEST, { measuredDeg: undefined });
	const left: SitesFiltered = {
		fitting: [slope],
		unsaid: [ramp],
		words: '1 of 2 take-offs fits a wind from NW and lies at 1500 m or higher',
		unsaidTitle: '1 take-off gives no wind directions or no height',
		unsaidNote: 'Their source does not say.',
		filterWords: 'Take-offs for a wind from NW, from 1500 m up',
		isOn: true
	};

	it('is the core’s answer, once it is for these sites and this filter', () => {
		expect(shownNow(sites, filter, NORTH_WEST, { sites, filter, left })).toEqual({
			fitting: [slope],
			unsaid: [ramp],
			left
		});
	});

	it('is every site until the core has answered, where nothing is chosen', () => {
		const nothing = asked(NO_CHOICE, { measuredDeg: undefined });

		expect(shownNow(sites, nothing, NO_CHOICE, undefined)).toEqual({
			fitting: sites,
			unsaid: [],
			left: undefined
		});
		// An answer for another filter says nothing about this one.
		expect(shownNow(sites, nothing, NO_CHOICE, { sites, filter, left }).fitting).toBe(sites);
	});

	it('is what the last answer left of the sites still held, while a filter is asked anew', () => {
		// The map moved: the same filter, other sites, one of them known.
		const moved = [slope, site({ id: 'test:node/9' })];

		expect(shownNow(moved, filter, NORTH_WEST, { sites, filter, left })).toEqual({
			fitting: [slope],
			unsaid: [],
			left: undefined
		});
		// Nothing was answered yet: no sign is drawn that the filter might leave out.
		expect(shownNow(sites, filter, NORTH_WEST, undefined)).toEqual({
			fitting: [],
			unsaid: [],
			left: undefined
		});
	});
});
