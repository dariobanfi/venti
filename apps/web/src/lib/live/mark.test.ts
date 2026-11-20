import { describe, expect, it } from 'vitest';
import { createFormat } from '#lib/design/format.ts';
import { countIn, gliderFeatures, listed, liveLayer } from './mark';
import { colourOf, lookOf, pictureName } from './symbols';
import { glider } from './testing';
import { NO_LIVE } from './watch';
import { detail, nobodyListed } from './words';

const text = createFormat('en-GB', 'Europe/Berlin');

describe('a glider on the map', () => {
	it('is a wing turned to its course, with the core’s line under it', () => {
		const [feature] = gliderFeatures([glider()]).features;

		expect(feature.geometry).toEqual({ type: 'Point', coordinates: [11.7432, 47.6638] });
		expect(feature.properties).toEqual({
			id: '11111111111111111111111111',
			picture: 'live-flying',
			heading: 20,
			label: 'Demo Pilot · 1596 m'
		});
	});

	it('is held back when nothing was heard of it for a while, and is not drawn as landed', () => {
		const quiet = glider({ quiet: true });

		expect(lookOf(quiet)).toBe('quiet');
		expect(gliderFeatures([quiet]).features[0].properties).toMatchObject({
			picture: 'live-quiet',
			heading: 20
		});
		// Grey like a glider on the ground, but still a wing.
		expect(colourOf('quiet')).toBe(colourOf('landed'));
		expect(colourOf('flying')).not.toBe(colourOf('quiet'));
		expect(pictureName('quiet')).not.toBe(pictureName('landed'));
	});

	it('is a disc that points nowhere once its pilot’s app has said that it landed', () => {
		// The last course before the landing is not where it is going.
		const landed = glider({ landed: true, courseDeg: 240 });

		expect(gliderFeatures([landed]).features[0].properties).toMatchObject({
			picture: 'live-landed',
			heading: 0
		});
		// Landed is said by the app; quiet beside it changes nothing.
		expect(lookOf(glider({ landed: true, quiet: true }))).toBe('landed');
	});

	it('points north when its app sent no course', () => {
		expect(gliderFeatures([glider({ courseDeg: undefined })]).features[0].properties).toMatchObject(
			{ picture: 'live-flying', heading: 0 }
		);
	});

	it('is never left out for want of room, though its label may be', () => {
		const layer = liveLayer('Noto Sans Regular');

		expect(layer).toMatchObject({
			type: 'symbol',
			layout: {
				'icon-allow-overlap': true,
				'text-optional': true,
				'icon-rotation-alignment': 'map'
			}
		});
	});
});

describe('the gliders in a list', () => {
	it('are those that fly first, then the quiet ones, then those on the ground, by name within each', () => {
		const gliders = [
			glider({ id: 'a', name: 'Zoe', landed: true }),
			glider({ id: 'b', name: 'Yann', quiet: true }),
			glider({ id: 'c', name: 'Xaver' }),
			glider({ id: 'd', name: 'anna' }),
			glider({ id: 'e', name: 'Bea', landed: true })
		];

		expect(listed(gliders).map((glider) => glider.name)).toEqual([
			'anna',
			'Xaver',
			'Yann',
			'Bea',
			'Zoe'
		]);
		// The server's order is left as it was.
		expect(gliders[0].name).toBe('Zoe');
	});

	it('keep two tracks of one pilot in a steady order', () => {
		const gliders = [glider({ id: '2' }), glider({ id: '1' })];

		expect(listed(gliders).map((glider) => glider.id)).toEqual(['1', '2']);
	});

	it('are counted for the part of the map in view', () => {
		const gliders = [
			glider({ position: { lat: 47.66, lon: 11.74 } }),
			glider({ position: { lat: 47.69, lon: 11.77 } }),
			glider({ position: { lat: 47.9, lon: 11.74 } })
		];

		expect(countIn(gliders, { south: 47.6, west: 11.7, north: 47.8, east: 11.9 })).toBe(2);
	});

	it('say how high, how it climbs and how old the position is', () => {
		expect(detail(glider(), 'Now', text)).toBe('1596 m · \u22121.0 m/s · Now');
		// A glider on the ground does not climb.
		expect(detail(glider({ landed: true, altitudeM: 786 }), 'Landed 9 min ago', text)).toBe(
			'786 m · Landed 9 min ago'
		);
		// What the app did not send is left out, and so is an age not known yet.
		expect(detail(glider({ climbMs: undefined }), '', text)).toBe('1596 m');
	});

	it('say why nobody is listed, and what would change that', () => {
		expect(nobodyListed({ ...NO_LIVE, close: true })).toEqual({
			title: 'Nobody in the air here',
			says: 'Pilots who show their flight to everyone appear on this part of the map while they fly.'
		});
		expect(nobodyListed(NO_LIVE).title).toBe('The map shows too much');
		expect(nobodyListed(NO_LIVE).says).toContain('Zoom the map in');
	});
});
