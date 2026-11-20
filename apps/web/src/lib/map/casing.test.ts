import { describe, expect, it } from 'vitest';
import type { LayerSpecification, LineLayerSpecification } from 'maplibre-gl';
import { addLine, CASED, CASING, casingOf, casingsOn } from './casing';

const line = (id: string, width = 2.5): LineLayerSpecification => ({
	id,
	type: 'line',
	source: 'somewhere',
	layout: { 'line-join': 'round', 'line-cap': 'round' },
	paint: { 'line-color': '#221640', 'line-width': width }
});

describe('the light casing under the app’s dark lines', () => {
	it('is given to the logbook’s tracks, a followed track, the scored route and the other flight', () => {
		expect(CASED).toEqual([
			'logbook-tracks-edge',
			'live-track-edge',
			'flight-route-line',
			'other-track-edge'
		]);
		const added: LayerSpecification[] = [];
		const map = { addLayer: (layer: LayerSpecification) => void added.push(layer) };
		for (const id of [...CASED, 'flight-track-edge', 'logbook-tracks-line'])
			addLine(map as never, line(id));
		expect(added.map((layer) => layer.id)).toEqual([
			`logbook-tracks-edge${CASING}`,
			'logbook-tracks-edge',
			`live-track-edge${CASING}`,
			'live-track-edge',
			`flight-route-line${CASING}`,
			'flight-route-line',
			`other-track-edge${CASING}`,
			'other-track-edge',
			// The flight's own track keeps its dark casing alone.
			'flight-track-edge',
			'logbook-tracks-line'
		]);
	});

	it('is 1.5 px wider on each side, in the label halo’s colour, solid, and hidden on the drawn map', () => {
		const dashed = { ...line('flight-route-line', 1.5) };
		dashed.paint = { ...dashed.paint, 'line-dasharray': [4, 3] };
		const casing = casingOf(dashed);
		expect(casing.paint).toEqual({ 'line-color': '#FFF8E8', 'line-width': 4.5 });
		expect(casing.layout?.visibility).toBe('none');
		expect(casing.source).toBe('somewhere');
	});

	it('is found on a map by its name', () => {
		const map = {
			getStyle: () => ({ layers: [{ id: `a${CASING}` }, { id: 'a' }, { id: 'b' }] })
		};
		expect(casingsOn(map as never)).toEqual([`a${CASING}`]);
	});
});
