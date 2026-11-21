import { describe, expect, it } from 'vitest';
import { otherById, otherFeatures } from './others';
import { aircraft, others } from './testing';

const towPlane = aircraft({
	id: '06c45574ebec0149',
	kind: 'towPlane',
	kindWords: 'Tow plane',
	isFreeFlight: false,
	courseDeg: undefined
});

describe('the aircraft other networks hear, on the map', () => {
	it('are drawn turned the way they fly, with the core’s label, and told apart by whether they fly free', () => {
		const { networks } = others(1, [aircraft(), towPlane]);

		const features = otherFeatures(networks).features;

		expect(features.map((feature) => feature.properties)).toEqual([
			{ id: '4791c887c4a52d11', heading: 210, label: 'Paraglider · 2140 m', free: true },
			// One that sent no course points north.
			{ id: '06c45574ebec0149', heading: 0, label: 'Paraglider · 2140 m', free: false }
		]);
		expect(features[0].geometry).toEqual({ type: 'Point', coordinates: [11.7967, 47.664083] });
	});

	it('are found by their identifier, with their network', () => {
		const { networks } = others(1, [aircraft(), towPlane]);

		expect(otherById(networks, '06c45574ebec0149')?.aircraft.kindWords).toBe('Tow plane');
		expect(otherById(networks, '06c45574ebec0149')?.network.source).toBe('ogn');
		expect(otherById(networks, 'gone')).toBeUndefined();
	});
});
