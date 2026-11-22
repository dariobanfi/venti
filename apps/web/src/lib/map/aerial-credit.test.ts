import { describe, expect, it } from 'vitest';
import { type Area, aerialMakers, photosCredit } from './aerial-credit';
import { mapStyle } from './style';

const where = { origin: 'https://venti.example' };

/** A part of the map about a valley wide, around a place. */
function around(lat: number, lon: number): Area {
	return { south: lat - 0.05, west: lon - 0.08, north: lat + 0.05, east: lon + 0.08 };
}

const TEGERNSEE = around(47.71, 11.75);
const INTERLAKEN = around(46.69, 7.86);
const GRENOBLE = around(45.19, 5.72);
/** Annecy lies inside the box of Switzerland's photographs, which are transparent there. */
const ANNECY = around(45.9, 6.13);
const ISNY = around(47.69, 10.04);
const FREIBURG = around(48.0, 7.85);

describe('the short line of whose the photographs are', () => {
	const makers = aerialMakers(mapStyle(where));

	it('reads every layer of photographs of the shared style, in its order', () => {
		expect(makers.map((maker) => maker.credit)).toEqual([
			'EOX',
			'swisstopo',
			'Autonome Provinz Bozen - Südtirol',
			'IGN',
			'basemap.at',
			'LGL-BW',
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
		expect(photosCredit(makers, INTERLAKEN, 11)).toBe('Photos: swisstopo · EOX');
		// France's box (Corsica to Alsace) takes in western Switzerland:
		// the line goes by the boxes, and names IGN there from zoom 13,
		// where its source has tiles.
		expect(photosCredit(makers, INTERLAKEN, 12)).toBe('Photos: swisstopo · EOX');
		expect(photosCredit(makers, INTERLAKEN, 13)).toBe('Photos: swisstopo · IGN · EOX');
		expect(photosCredit(makers, FREIBURG, 11)).toBe('Photos: LGL-BW · EOX');
		expect(photosCredit(makers, ISNY, 14)).toBe(
			'Photos: swisstopo · basemap.at · LGL-BW · Bayerische Vermessungsverwaltung · EOX'
		);
	});

	it('names France’s photographs from zoom 13, where their source has tiles, inside their box', () => {
		// The layer is drawn from 12, but its source has no tiles before 13.
		expect(makers.find((maker) => maker.credit === 'IGN')?.minzoom).toBe(13);
		expect(photosCredit(makers, GRENOBLE, 11)).toBe('Photos: EOX');
		expect(photosCredit(makers, GRENOBLE, 12)).toBe('Photos: EOX');
		expect(photosCredit(makers, GRENOBLE, 13)).toBe('Photos: IGN · EOX');
		expect(photosCredit(makers, ANNECY, 15)).toBe('Photos: swisstopo · IGN · EOX');
		expect(photosCredit(makers, TEGERNSEE, 15)).not.toContain('IGN');
	});

	it('names only the world’s before the zoom the regions’ photographs show from', () => {
		expect(photosCredit(makers, TEGERNSEE, 8)).toBe('Photos: EOX');
		expect(photosCredit(makers, INTERLAKEN, 8)).toBe('Photos: EOX');
		expect(photosCredit(makers, around(46.5, 11.35), 8)).toBe('Photos: EOX');
		expect(photosCredit(makers, FREIBURG, 10)).toBe('Photos: EOX');
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
