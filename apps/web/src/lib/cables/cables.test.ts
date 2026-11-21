import { describe, expect, it } from 'vitest';
import type { CableLine, CablesMap, Coordinate } from '#lib/core/index.ts';
import { CablesModel } from './model.svelte';
import { cableFeatures, pixelsFromLine } from './on-map';

const line = (id: string, heightMapped: boolean): CableLine => ({
	id,
	kind: 'powerLine',
	points: [
		{ lat: 47.7, lon: 11.7 },
		{ lat: 47.71, lon: 11.72 }
	],
	heightMapped,
	words: 'Power line, height not mapped',
	name: undefined
});

const LAYER: CablesMap = {
	lines: [line('osm-cables:way/1', false), line('osm-cables:way/2', true)],
	credits: [],
	limits: 'No promise.'
};

const box = (lat: number, lon: number) => ({
	south: lat - 0.05,
	west: lon - 0.05,
	north: lat + 0.05,
	east: lon + 0.05
});

describe('the cables on the map', () => {
	it('are lines, longitude first, dashed where their height is not mapped', () => {
		const features = cableFeatures(LAYER.lines).features;
		expect(features.map((feature) => feature.properties)).toEqual([
			{ id: 'osm-cables:way/1', dashed: true },
			{ id: 'osm-cables:way/2', dashed: false }
		]);
		expect((features[0].geometry as GeoJSON.LineString).coordinates[0]).toEqual([11.7, 47.7]);
		expect(cableFeatures(undefined).features).toEqual([]);
	});

	it('take a click by how far it is from the line', () => {
		const line = [
			{ x: 0, y: 0 },
			{ x: 100, y: 0 }
		];
		expect(pixelsFromLine({ x: 50, y: 10 }, line)).toBe(10);
		expect(pixelsFromLine({ x: 130, y: 40 }, line)).toBe(50);
	});

	it('are asked for once the map rests close enough, and again only once it has moved far', async () => {
		const asked: Coordinate[] = [];
		const model = new CablesModel(async (place) => {
			asked.push(place);
			return LAYER;
		});
		model.show(box(47.7, 11.76), 8);
		expect(asked).toEqual([]);
		model.show(box(47.7, 11.76), 10);
		await Promise.resolve();
		expect(asked).toHaveLength(1);
		expect(model.lines).toHaveLength(2);
		model.show(box(47.75, 11.8), 11);
		expect(asked).toHaveLength(1);
		model.show(box(47.9, 11.8), 11);
		expect(asked).toHaveLength(2);
		// Zoomed out, nothing is drawn, and the answer is kept.
		model.show(box(47.9, 11.8), 7);
		expect(model.lines).toEqual([]);
		expect(model.answer).toBe(LAYER);
	});
});
