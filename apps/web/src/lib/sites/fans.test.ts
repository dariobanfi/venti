import { describe, expect, it } from 'vitest';
import type { SiteView } from '#lib/core/index.ts';
import { CAUTION_ON_MAP, GOOD_ON_MAP, INK_ON_MAP } from '#lib/design/palette.ts';
import { fanFeatures, fanName, fanOf, fitColour, fitOf, groundWinds, windFits } from './fans';

const NORTH_TO_SOUTHWEST = [{ fromDeg: 0, toDeg: 225 }];
const THROUGH_NORTH = [{ fromDeg: 315, toDeg: 45 }];
const SOUTH = [{ fromDeg: 180, toDeg: 180 }];

function site(id: string, uses: SiteView['uses'], wind: SiteView['wind']): SiteView {
	return {
		id,
		name: id,
		isNamed: true,
		kind: uses[0],
		uses,
		kindInWords: 'Take-off',
		position: { lat: 47.5, lon: 11.5 },
		elevationM: 1500,
		wind,
		windInWords: '',
		official: undefined,
		discouraged: false,
		paragliding: true,
		hangGliding: undefined,
		source: 'osm'
	} as unknown as SiteView;
}

/** The core's arrows of an hour: a point per square, pointing where the wind blows. */
function arrows(...squares: [lat: number, lon: number, heading: number | 'calm'][]) {
	return {
		type: 'FeatureCollection' as const,
		features: squares.map(([lat, lon, heading]) => ({
			type: 'Feature' as const,
			geometry: { type: 'Point' as const, coordinates: [lon, lat] },
			properties:
				heading === 'calm'
					? { cell: 0, symbol: 'calm', band: '0', heading: 0, label: '' }
					: { cell: 0, symbol: 'arrow', band: '2', heading, label: '12' }
		}))
	};
}

describe('a take-off’s fan', () => {
	it('fits a wind from within its spans, through north too, and a single direction’s sixteenth', () => {
		expect(windFits(NORTH_TO_SOUTHWEST, 90)).toBe(true);
		expect(windFits(NORTH_TO_SOUTHWEST, 270)).toBe(false);
		expect(windFits(THROUGH_NORTH, 350)).toBe(true);
		expect(windFits(THROUGH_NORTH, 10)).toBe(true);
		expect(windFits(THROUGH_NORTH, 180)).toBe(false);
		expect(windFits(SOUTH, 190)).toBe(true);
		expect(windFits(SOUTH, 205)).toBe(false);
		expect(windFits([{ fromDeg: 0, toDeg: 360 }], 123)).toBe(true);
	});

	it('reads the wind of the square a take-off lies in from the core’s arrows, and nothing far from any', () => {
		// An arrow pointing south is a wind from the north.
		const at = groundWinds(arrows([47.5, 11.5, 180], [47.6, 11.5, 'calm']));
		expect(at(47.51, 11.49)).toEqual({ fromDeg: 0 });
		expect(at(47.59, 11.5)).toBe('calm');
		expect(at(48.5, 11.5)).toBeUndefined();
		expect(groundWinds(undefined)(47.5, 11.5)).toBeUndefined();
	});

	it('is the pack’s good colour where the wind fits, caution where not, and neutral without one', () => {
		expect(fitOf(SOUTH, { fromDeg: 180 })).toBe('fits');
		expect(fitOf(SOUTH, { fromDeg: 0 })).toBe('misfits');
		expect(fitOf(SOUTH, 'calm')).toBe('neutral');
		expect(fitOf(SOUTH, undefined)).toBe('neutral');
		expect(fitColour('fits')).toBe(GOOD_ON_MAP);
		expect(fitColour('misfits')).toBe(CAUTION_ON_MAP);
		expect(fitColour('neutral')).toBe(INK_ON_MAP);
	});

	it('names its picture by what it says and its spans, and reads them back', () => {
		const name = fanName([...THROUGH_NORTH, ...SOUTH], 'fits');
		expect(name).toBe('site-fan:fits:315-45,180-180');
		expect(fanOf(name)).toEqual({ fit: 'fits', spans: [...THROUGH_NORTH, ...SOUTH] });
		expect(fanOf('site-takeoff')).toBeUndefined();
		expect(fanOf('site-fan:other:0-90')).toBeUndefined();
	});

	it('is drawn for take-offs with directions only: a landing has its own sign', () => {
		const sites = [
			site('ramp', ['takeoff'], SOUTH),
			site('both', ['takeoff', 'landing'], NORTH_TO_SOUTHWEST),
			site('field', ['landing'], SOUTH),
			site('unknown', ['takeoff'], [])
		];
		const plain = fanFeatures(sites);
		expect(plain.features.map((fan) => fan.properties?.id)).toEqual(['ramp', 'both']);
		expect(plain.features[0].properties?.picture).toBe('site-fan:neutral:180-180');
		// With the forecast’s wind from the west at the ground.
		const coloured = fanFeatures(sites, () => ({ fromDeg: 270 }));
		expect(coloured.features.map((fan) => fan.properties?.picture)).toEqual([
			'site-fan:misfits:180-180',
			'site-fan:misfits:0-225'
		]);
		const fromSouth = fanFeatures(sites, () => ({ fromDeg: 180 }));
		expect(fromSouth.features.map((fan) => fan.properties?.picture)).toEqual([
			'site-fan:fits:180-180',
			'site-fan:fits:0-225'
		]);
	});
});
