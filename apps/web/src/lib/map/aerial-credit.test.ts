import { describe, expect, it } from 'vitest';
import { type Area, aerialMakers, photosCredit } from './aerial-credit';
import { mapStyle } from './style';

const where = { origin: 'https://venti.example', hostname: 'venti.example' };

/** A part of the map about a valley wide, around a place. */
function around(lat: number, lon: number): Area {
	return { south: lat - 0.05, west: lon - 0.08, north: lat + 0.05, east: lon + 0.08 };
}

const TEGERNSEE = around(47.71, 11.75);
const INTERLAKEN = around(46.69, 7.86);

describe('the short line of whose the photographs are', () => {
	const makers = aerialMakers(mapStyle(where));

	it('reads every layer of photographs of the shared style, in its order', () => {
		expect(makers.map((maker) => maker.credit)).toEqual([
			'EOX',
			'swisstopo',
			'Autonome Provinz Bozen - Südtirol',
			'basemap.at',
			'Bayerische Vermessungsverwaltung'
		]);
		// The world's photographs have no bounds; the others have theirs from the source.
		expect(makers[0].area).toBeUndefined();
		expect(makers[1].area).toEqual({ west: 5.9395, south: 45.8061, east: 10.519, north: 47.8095 });
	});

	it('names the makers whose photographs show where the map is, the world’s last', () => {
		expect(photosCredit(makers, TEGERNSEE, 12)).toBe(
			'Photos: basemap.at · Bayerische Vermessungsverwaltung · EOX'
		);
		expect(photosCredit(makers, INTERLAKEN, 12)).toBe('Photos: swisstopo · EOX');
	});

	it('names only the world’s before the zoom the regions’ photographs show from', () => {
		expect(photosCredit(makers, TEGERNSEE, 8)).toBe('Photos: EOX');
		expect(photosCredit(makers, INTERLAKEN, 8)).toBe('Photos: EOX');
		expect(photosCredit(makers, around(46.5, 11.35), 8)).toBe('Photos: EOX');
	});

	it('does not name a maker whose layer the loaded style lacks', () => {
		const style = mapStyle(where);
		style.layers = style.layers.filter((layer) => layer.id !== 'aerial-austria');

		expect(photosCredit(aerialMakers(style), TEGERNSEE, 12)).toBe(
			'Photos: Bayerische Vermessungsverwaltung · EOX'
		);
	});

	it('names each maker once, and nothing where no photographs show', () => {
		const twice = [
			{ credit: 'EOX', minzoom: 0 },
			{ credit: 'EOX', minzoom: 0 }
		];

		expect(photosCredit(twice, TEGERNSEE, 3)).toBe('Photos: EOX');
		expect(photosCredit(makers.slice(1), TEGERNSEE, 8)).toBe('');
	});
});
