import { describe, expect, it } from 'vitest';
import type { FlightMoment, FlightScore, TrackPoint } from '#lib/core/index.ts';
import { boundsAround, outlineLines, place, routeLines, trackByBand } from './shapes';

function track(climbs: number[]): TrackPoint[] {
	return climbs.map((climbMs, index) => ({
		lat: 47 + index,
		lon: 11 + index / 10,
		altitudeM: 1000,
		seconds: index,
		climbMs
	}));
}

describe('a track coloured by climb', () => {
	it('is one shape per band, with every stretch flown in that band', () => {
		// Sink, a strong climb, then sink again.
		const shapes = trackByBand(track([0, -1, -1, 3, 3, -1]));
		const byBand = new Map(
			shapes.features.map((feature) => [feature.properties.band, feature.geometry.coordinates])
		);

		expect([...byBand.keys()].sort()).toEqual([1, 4]);
		// Two stretches of sink, which the climb between them joins end to end.
		expect(byBand.get(1)).toEqual([
			[
				[11, 47],
				[11.1, 48],
				[11.2, 49]
			],
			[
				[11.4, 51],
				[11.5, 52]
			]
		]);
		expect(byBand.get(4)).toEqual([
			[
				[11.2, 49],
				[11.3, 50],
				[11.4, 51]
			]
		]);
	});

	it('is nothing for a track too short to draw', () => {
		expect(trackByBand(track([1])).features).toEqual([]);
	});
});

describe('the outlines of the logbook’s flights', () => {
	const short = [{ lat: 47, lon: 11 }];
	const first = [
		{ lat: 47.6, lon: 11.7 },
		{ lat: 47.7, lon: 11.8 }
	];
	const second = [
		{ lat: 46.5, lon: 12.1 },
		{ lat: 46.4, lon: 10.9 }
	];

	it('are drawn as lines with longitude first, leaving out what is no line', () => {
		expect(outlineLines([first, short, second]).geometry.coordinates).toEqual([
			[
				[11.7, 47.6],
				[11.8, 47.7]
			],
			[
				[12.1, 46.5],
				[10.9, 46.4]
			]
		]);
	});

	it('are all inside the rectangle the map is fitted to', () => {
		expect(boundsAround([first, second])).toEqual({
			south: 46.4,
			west: 10.9,
			north: 47.7,
			east: 12.1
		});
		expect(boundsAround([short])).toEqual({ south: 47, west: 11, north: 47, east: 11 });
	});

	it('have no rectangle when there are none', () => {
		expect(boundsAround([])).toBeUndefined();
		expect(boundsAround([[]])).toBeUndefined();
	});
});

describe('the scored route', () => {
	const moment = (lon: number): FlightMoment => ({
		trackIndex: lon,
		seconds: lon,
		position: { lat: 47, lon },
		altitudeM: 1000
	});
	const score = (kind: FlightScore['kind'], lons: number[]): FlightScore => ({
		kind,
		closed: false,
		distanceM: 0,
		points: 0,
		pointsPerKm: 1,
		route: lons.map(moment),
		closingM: undefined
	});

	it('is one line through every point of a free flight', () => {
		expect(routeLines(score('freeFlight', [1, 2, 3, 4, 5])).geometry.coordinates).toEqual([
			[
				[1, 47],
				[2, 47],
				[3, 47],
				[4, 47],
				[5, 47]
			]
		]);
	});

	it('is the closed triangle, with the way to it and the way from it', () => {
		for (const kind of ['freeTriangle', 'faiTriangle'] as const) {
			expect(routeLines(score(kind, [1, 2, 3, 4, 5])).geometry.coordinates).toEqual([
				[
					[1, 47],
					[2, 47]
				],
				[
					[2, 47],
					[3, 47],
					[4, 47],
					[2, 47]
				],
				[
					[4, 47],
					[5, 47]
				]
			]);
		}
	});
});

describe('a place', () => {
	it('is a point with longitude first', () => {
		expect(place({ lat: 47.6, lon: 11.7 }).geometry.coordinates).toEqual([11.7, 47.6]);
	});
});
