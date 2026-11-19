import { describe, expect, it } from 'vitest';
import { countIn, listed, siteFeatures, sitesLayer, SITES_SOURCE } from './mark';
import { wedges } from './rose';
import { colours, pictureName, SIGNS, SITE_KINDS } from './symbols';
import { site } from './testing';
import { SITES_MIN_ZOOM } from './watch';

const JAEGERKAMP = site({
	id: 'test:way/1',
	name: 'Fluggelände Jägerkamp',
	isNamed: true,
	position: { lat: 47.679, lon: 11.905 }
});
const LANDING = site({
	id: 'test:way/2',
	name: 'Landeplatz Jägerkamp',
	isNamed: true,
	kind: 'landing',
	uses: ['landing'],
	kindInWords: 'Landing',
	position: { lat: 47.692, lon: 11.887 }
});
const RAMP = site({ id: 'test:node/3', position: { lat: 47.665, lon: 11.795 } });

describe('flying sites on the map', () => {
	it('are signs by what they are, with a name only where the source gives one', () => {
		const { features } = siteFeatures([JAEGERKAMP, LANDING, RAMP]);

		expect(features.map((feature) => feature.properties)).toEqual([
			{ id: 'test:way/1', picture: 'site-takeoff', label: 'Fluggelände Jägerkamp' },
			{ id: 'test:way/2', picture: 'site-landing', label: 'Landeplatz Jägerkamp' },
			// "Take-off" under every take-off sign would say nothing.
			{ id: 'test:node/3', picture: 'site-takeoff', label: '' }
		]);
		expect(features[0].geometry).toEqual({ type: 'Point', coordinates: [11.905, 47.679] });
	});

	it('are a layer of their own, drawn once the map shows a valley', () => {
		const layer = sitesLayer('noto-sans-regular');

		expect(layer).toMatchObject({
			id: SITES_SOURCE,
			source: SITES_SOURCE,
			minzoom: SITES_MIN_ZOOM
		});
		// Two ramps on one mountain are both drawn; a name gives way.
		expect(layer.layout).toMatchObject({ 'icon-allow-overlap': true, 'text-optional': true });
	});

	it('have a picture for every kind, the landing light and the rest dark', () => {
		expect(SITE_KINDS.map(pictureName)).toEqual([
			'site-takeoff',
			'site-landing',
			'site-towing',
			'site-training',
			'site-other'
		]);
		for (const kind of SITE_KINDS) expect(SIGNS[kind]).toMatch(/^M/);
		expect(colours('landing')).toEqual({ ground: '#ffffff', sign: '#1F1F1D' });
		expect(colours('takeoff')).toEqual({ ground: '#1F1F1D', sign: '#ffffff' });
		expect(colours('other')).toEqual(colours('takeoff'));
	});

	it('are counted where the map shows them', () => {
		const valley = { south: 47.6, west: 11.85, north: 47.75, east: 11.95 };

		expect(countIn([JAEGERKAMP, LANDING, RAMP], valley)).toBe(2);
		expect(countIn([], valley)).toBe(0);
	});
});

describe('flying sites in a list', () => {
	it('come with the named ones first, by name, then the rest by what they are', () => {
		const unnamedLanding = site({ id: 'test:node/4', name: 'Landing', kind: 'landing' });

		expect(listed([RAMP, LANDING, unnamedLanding, JAEGERKAMP]).map((site) => site.id)).toEqual([
			'test:way/1',
			'test:way/2',
			'test:node/4',
			'test:node/3'
		]);
	});

	it('are found by a part of their name, whatever its case', () => {
		expect(listed([RAMP, LANDING, JAEGERKAMP], ' LANDEPLATZ ').map((site) => site.id)).toEqual([
			'test:way/2'
		]);
		expect(listed([RAMP, LANDING, JAEGERKAMP], 'nothing like this')).toEqual([]);
	});
});

describe('the compass of a site', () => {
	it('draws a span as a wedge, clockwise from its first direction', () => {
		// South to west on a circle of radius 10 around (15, 15).
		expect(wedges([{ fromDeg: 180, toDeg: 270 }], 15, 10)).toEqual([
			'M15 15L15 25A10 10 0 0 1 5 15Z'
		]);
	});

	it('draws a single direction as the sixteenth of the compass it names', () => {
		const [north] = wedges([{ fromDeg: 0, toDeg: 0 }], 15, 10);

		// From 11.25° west of north to 11.25° east of it.
		expect(north).toBe('M15 15L13.05 5.19A10 10 0 0 1 16.95 5.19Z');
	});

	it('follows a span through north, however it is written', () => {
		const throughNorth = 'M15 15L7.93 7.93A10 10 0 0 1 22.07 7.93Z';

		expect(wedges([{ fromDeg: 315, toDeg: 45 }], 15, 10)).toEqual([throughNorth]);
		expect(wedges([{ fromDeg: 315, toDeg: 405 }], 15, 10)).toEqual([throughNorth]);
	});

	it('takes the long way round for a span of more than half the compass', () => {
		const [wide] = wedges([{ fromDeg: 90, toDeg: 315 }], 15, 10);

		expect(wide).toBe('M15 15L25 15A10 10 0 1 1 7.93 7.93Z');
	});

	it('fills the disc for any direction, and draws nothing for none', () => {
		expect(wedges([{ fromDeg: 0, toDeg: 360 }], 15, 10)).toEqual([
			'M15 5A10 10 0 1 1 15 25A10 10 0 1 1 15 5Z'
		]);
		expect(wedges([], 15, 10)).toEqual([]);
	});
});
