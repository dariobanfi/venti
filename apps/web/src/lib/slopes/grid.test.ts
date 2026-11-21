import { describe, expect, it } from 'vitest';
import { SLOPE_ON_MAP } from '#lib/design/palette.ts';
import { cornersOf, gridFor, MAX_CELLS, SLOPE_KEY, slopeCounts, slopePixels } from './grid';

/** A box of `widthPx` by `heightPx` pixels around the Wallberg at a zoom. */
function screen(widthPx: number, heightPx: number, zoom: number) {
	const world = 512 * 2 ** zoom;
	const dLon = (widthPx / world) * 360;
	// Near enough for a box this small: a degree of latitude is 1/cos of one of longitude on the map.
	const dLat = dLon * (heightPx / widthPx) * Math.cos((47.66 * Math.PI) / 180);
	return {
		south: 47.66 - dLat / 2,
		west: 11.8 - dLon / 2,
		north: 47.66 + dLat / 2,
		east: 11.8 + dLon / 2
	};
}

describe('the grid of the map of slopes', () => {
	it('has at most 40,000 cells on a desktop’s screen, some 5 pixels each', () => {
		const { columns, rows } = gridFor(screen(1280, 800, 13), 13);
		expect(columns * rows).toBeLessThanOrEqual(MAX_CELLS);
		expect(columns * rows).toBeGreaterThan(MAX_CELLS * 0.9);
		expect(1280 / columns).toBeCloseTo(5.06, 1);
	});

	it('has cells of 4 pixels on a phone, and of 25 m close in', () => {
		const phone = gridFor(screen(390, 844, 13), 13);
		expect(390 / phone.columns).toBeCloseTo(4, 0);
		// At zoom 14 a pixel is some 3.2 m: a cell of 25 m is some 8 pixels.
		const close = gridFor(screen(390, 844, 14), 14);
		expect(390 / close.columns).toBeGreaterThan(7);
		expect(390 / close.columns).toBeLessThan(9);
	});

	it('has no cells for a box that is none', () => {
		expect(gridFor({ south: 47, west: 11, north: 47, east: 11 }, 13)).toEqual({
			columns: 0,
			rows: 0
		});
	});
});

describe('the picture of the slopes', () => {
	it('colours each class by the thermal scale’s steps from light to strong, and leaves cells without ground clear', () => {
		const pixels = slopePixels(new Uint8Array([0, 1, 5]));
		expect([...pixels.slice(0, 4)]).toEqual([0, 0, 0, 0]);
		const hex = (index: number) =>
			'#' +
			[...pixels.slice(index * 4, index * 4 + 3)]
				.map((one) => one.toString(16).padStart(2, '0'))
				.join('')
				.toUpperCase();
		expect(hex(1)).toBe(SLOPE_ON_MAP[0].toUpperCase());
		expect(hex(2)).toBe(SLOPE_ON_MAP[4].toUpperCase());
		expect(pixels[7]).toBe(255);
	});

	it('counts the cells of each class', () => {
		expect(slopeCounts(new Uint8Array([0, 1, 1, 3, 5]))).toEqual({
			classed: 4,
			byClass: [2, 0, 1, 0, 1]
		});
	});

	it('has a key of five classes in the map’s colours', () => {
		expect(SLOPE_KEY.map((entry) => entry.words)).toEqual([
			'Under 25°',
			'25–30°',
			'30–35°',
			'35–40°',
			'40° and over'
		]);
		expect(SLOPE_KEY.map((entry) => entry.color)).toEqual(SLOPE_ON_MAP);
	});

	it('is pinned by its corners from the north-west round', () => {
		expect(cornersOf({ south: 1, west: 2, north: 3, east: 4 })).toEqual([
			[2, 3],
			[4, 3],
			[4, 1],
			[2, 1]
		]);
	});
});
