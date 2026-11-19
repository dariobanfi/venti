import { describe, expect, it } from 'vitest';
import { WIND_BANDS } from '#lib/design/palette.ts';
import { compassPoint } from '#lib/design/format.ts';
import { pictureName, spoken, stationFeatures } from './mark';
import { mark, OLD, reading, station } from './testing';

/** Metres per second for a wind given in km/h. */
const kmh = (speed: number) => speed / 3.6;

describe('the look of the bands of wind', () => {
	it('is one blue from light to dark, the arrow growing with the wind', () => {
		const bands = [WIND_BANDS.light, WIND_BANDS.moderate, WIND_BANDS.fresh, WIND_BANDS.strong];

		expect(bands.map((band) => band.color)).toEqual(['#86B6EF', '#3987E5', '#1C5CAB', '#0D366B']);
		const scales = bands.map((band) => band.arrowScale);
		expect(scales).toEqual(scales.toSorted((first, second) => first - second));
	});
});

describe('the stations as the map reads them', () => {
	const features = stationFeatures([
		station({
			id: 'a:1',
			latest: reading({ speedMs: kmh(8), gustMs: kmh(15) }),
			mark: mark({ symbol: { kind: 'arrow', band: 'light' }, label: '8/15' })
		}),
		station({
			id: 'a:2',
			latest: reading({ speedMs: kmh(35), gustMs: kmh(60) }),
			mark: mark({ symbol: { kind: 'arrow', band: 'strong' }, headingDeg: 315, label: '35/60' })
		}),
		station({ id: 'a:3', isOld: true, mark: OLD }),
		station({
			id: 'a:4',
			latest: reading({ speedMs: kmh(12), gustMs: undefined }),
			mark: mark({ label: '12' })
		}),
		station({
			id: 'a:5',
			latest: reading({ speedMs: 0.2, gustMs: 1.4 }),
			mark: mark({ symbol: { kind: 'calm' }, headingDeg: 0, label: '1/5' })
		})
	]).features;
	const properties = features.map((feature) => feature.properties!);

	it('stand where the stations stand, longitude first', () => {
		expect(features[0].geometry).toEqual({ type: 'Point', coordinates: [11.5, 47.5] });
	});

	it('carry the picture, heading, size and label the core gave each', () => {
		expect(properties[0]).toMatchObject({
			id: 'a:1',
			picture: 'wind-arrow-light',
			heading: 90,
			scale: 0.85,
			label: '8/15'
		});
		expect(properties[1]).toMatchObject({ picture: 'wind-arrow-strong', heading: 315, scale: 1.3 });
		expect(properties[2]).toMatchObject({ picture: 'wind-old', scale: 1, label: '' });
		expect(properties[4]).toMatchObject({ picture: 'wind-calm', scale: 1, label: '1/5' });
	});

	it('rank the gustiest first and a station with nothing to say last', () => {
		const byRank = properties
			.toSorted((first, second) => first.rank - second.rank)
			.map((station) => station.id);

		expect(byRank).toEqual(['a:2', 'a:1', 'a:4', 'a:5', 'a:3']);
	});

	it('name a picture for every symbol', () => {
		expect(pictureName({ kind: 'arrow', band: 'moderate' })).toBe('wind-arrow-moderate');
		expect(pictureName({ kind: 'calm' })).toBe('wind-calm');
		expect(pictureName({ kind: 'old' })).toBe('wind-old');
	});
});

describe('a station in a sentence', () => {
	it('says wind, gusts and where from', () => {
		const said = spoken(
			station({ latest: reading({ speedMs: kmh(25), gustMs: kmh(41), directionDeg: 270 }) }),
			compassPoint
		);

		expect(said).toBe('Wind 25 kilometres per hour, gusts 41, from W');
	});

	it('leaves the direction out of air the core calls calm', () => {
		const calm = station({
			latest: reading({ speedMs: 0.2, gustMs: 1.4 }),
			mark: mark({ symbol: { kind: 'calm' }, headingDeg: 0, label: '1/5' })
		});

		expect(spoken(calm, compassPoint)).toBe('Wind 1 kilometre per hour, gusts 5');
	});

	it('says so when the core has nothing current to draw', () => {
		expect(spoken(station({ isOld: true, mark: OLD }), compassPoint)).toBe('No current reading');
	});
});
