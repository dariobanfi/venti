import { describe, expect, it } from 'vitest';
import { WIND_BANDS, windBand } from '#lib/design/palette.ts';
import { compassPoint } from '#lib/design/format.ts';
import { pictureName, speeds, spoken, stationFeatures, windMark } from './mark';
import { reading, station } from './testing';

/** Metres per second for a wind given in km/h. */
const kmh = (speed: number) => speed / 3.6;

describe('the bands of wind', () => {
	it('are four, at round numbers of km/h', () => {
		expect([0, 9.9, 10, 19.9, 20, 29.9, 30, 80].map((speed) => windBand(kmh(speed)))).toEqual([
			0, 0, 1, 1, 2, 2, 3, 3
		]);
	});

	it('are one blue from light to dark, the arrow growing with the wind', () => {
		expect(WIND_BANDS.map((band) => band.color)).toEqual([
			'#86B6EF',
			'#3987E5',
			'#1C5CAB',
			'#0D366B'
		]);
		const scales = WIND_BANDS.map((band) => band.arrowScale);
		expect(scales).toEqual(scales.toSorted((first, second) => first - second));
	});
});

describe('how a station is drawn', () => {
	it('is an arrow pointing where the wind blows, in the colour of its strength', () => {
		const mark = windMark(
			station({ latest: reading({ speedMs: kmh(25), gustMs: kmh(41), directionDeg: 270 }) })
		);

		expect(mark.symbol).toEqual({ kind: 'arrow', band: 2 });
		// West wind blows towards the east.
		expect(mark.heading).toBe(90);
		expect(mark.label).toBe('25/41');
	});

	it('turns a wind from the south-east to point north-west', () => {
		expect(windMark(station({ latest: reading({ directionDeg: 135 }) })).heading).toBe(315);
		expect(windMark(station({ latest: reading({ directionDeg: 350 }) })).heading).toBe(170);
	});

	it('is a dot with its numbers when the wind is too weak to have a direction', () => {
		const mark = windMark(station({ latest: reading({ speedMs: 0.2, gustMs: 1.4 }) }));

		expect(mark.symbol).toEqual({ kind: 'calm' });
		expect(mark.label).toBe('1/5');
	});

	it('is a dot when the station gives no direction', () => {
		const mark = windMark(station({ latest: reading({ directionDeg: undefined }) }));

		expect(mark.symbol).toEqual({ kind: 'calm' });
	});

	it('is a grey dot without numbers when the core calls the reading old', () => {
		const mark = windMark(station({ isOld: true, ageMin: 185 }));

		expect(mark).toEqual({ symbol: { kind: 'old' }, heading: 0, label: '' });
	});

	it('is a grey dot without numbers when there is no reading, or none of the wind', () => {
		expect(windMark(station({ latest: undefined, ageMin: undefined, isOld: true })).symbol).toEqual(
			{
				kind: 'old'
			}
		);
		expect(windMark(station({ latest: reading({ speedMs: undefined }) })).symbol).toEqual({
			kind: 'old'
		});
	});
});

describe('mean wind and gusts in a label', () => {
	it('are whole km/h with a stroke between them', () => {
		expect(speeds(14.9, 25.7)).toBe('54/93');
	});

	it('leave the gusts out when they are not known', () => {
		expect(speeds(kmh(12), undefined)).toBe('12');
	});

	it('leave the gusts out when they round to the same number', () => {
		expect(speeds(kmh(12), kmh(12.4))).toBe('12');
	});
});

describe('the stations as the map reads them', () => {
	const features = stationFeatures([
		station({ id: 'a:1', latest: reading({ speedMs: kmh(8), gustMs: kmh(15) }) }),
		station({ id: 'a:2', latest: reading({ speedMs: kmh(35), gustMs: kmh(60) }) }),
		station({ id: 'a:3', isOld: true }),
		station({ id: 'a:4', latest: reading({ speedMs: kmh(12), gustMs: undefined }) })
	]).features;
	const properties = features.map((feature) => feature.properties!);

	it('stand where the stations stand, longitude first', () => {
		expect(features[0].geometry).toEqual({ type: 'Point', coordinates: [11.5, 47.5] });
	});

	it('carry the picture, heading, size and label of each', () => {
		expect(properties[0]).toMatchObject({
			id: 'a:1',
			picture: 'wind-arrow-0',
			heading: 90,
			scale: 0.85,
			label: '8/15'
		});
		expect(properties[1]).toMatchObject({ picture: 'wind-arrow-3', scale: 1.3 });
		expect(properties[2]).toMatchObject({ picture: 'wind-old', scale: 1, label: '' });
	});

	it('rank the gustiest first and a station with nothing to say last', () => {
		const byRank = properties
			.toSorted((first, second) => first.rank - second.rank)
			.map((station) => station.id);

		expect(byRank).toEqual(['a:2', 'a:1', 'a:4', 'a:3']);
	});

	it('name a picture for every symbol', () => {
		expect(pictureName({ kind: 'arrow', band: 1 })).toBe('wind-arrow-1');
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

	it('leaves the direction out of calm air', () => {
		expect(spoken(station({ latest: reading({ speedMs: 0.2, gustMs: 1.4 }) }), compassPoint)).toBe(
			'Wind 1 kilometres per hour, gusts 5'
		);
	});

	it('says so when there is nothing current', () => {
		expect(spoken(station({ isOld: true }), compassPoint)).toBe('No current reading');
	});
});
