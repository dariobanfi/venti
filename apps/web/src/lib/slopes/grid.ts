/**
 * The map of slopes on the map tab: how fine a grid of places the core is
 * asked about for the part of the map in view, the five classes with their
 * colours and words, and the picture the map lays over the ground.
 *
 * A cell is at least `MIN_CELL_PX` pixels and `MIN_CELL_M` metres wide,
 * and a screen has at most `MAX_CELLS` of them: on a desktop's screen at
 * zoom 13 a cell is some 5 pixels, about 30 m; on a phone 4 pixels. The
 * core reads a place's steepness over 50 m, so a finer grid would say
 * nothing more. The picture is one pixel a cell, drawn without smoothing,
 * so a cell stays a small square.
 */

import type { Bounds } from '#lib/core/index.ts';
import { SLOPE_ON_MAP } from '#lib/design/palette.ts';

/** From this zoom on the map draws the slopes; further out the cells would be wider than a slope. */
export const SLOPES_MIN_ZOOM = 12;

/** At most this many cells on a screen. */
export const MAX_CELLS = 40_000;

/** A cell is at least this many pixels wide. */
export const MIN_CELL_PX = 4;

/** A cell is at least this many metres wide: half the stretch the core reads a slope over. */
export const MIN_CELL_M = 25;

/**
 * How much of the colour shows. The pack's fill for an area is 12–16 %,
 * but at that the five classes could not be told apart over the relief's
 * shading in the screenshots; 30 % is what the forecast's squares take at
 * this zoom (`squareOpacity`).
 */
export const SLOPES_OPACITY = 0.3;

/** The pack's tiles are 512 pixels at zoom 0. */
const TILE_PX = 512;
const EARTH_CIRCUMFERENCE_M = 40_075_016.686;

/** Where a latitude lies on the Mercator map, from 0 at the top to 1 at the bottom. */
function mercatorY(lat: number): number {
	const sin = Math.sin((lat * Math.PI) / 180);
	return 0.5 - Math.log((1 + sin) / (1 - sin)) / (4 * Math.PI);
}

/** How many cells across and down the grid over a box seen at a zoom has. */
export function gridFor(box: Bounds, zoom: number): { columns: number; rows: number } {
	const world = TILE_PX * 2 ** zoom;
	const widthPx = ((box.east - box.west) / 360) * world;
	const heightPx = (mercatorY(box.south) - mercatorY(box.north)) * world;
	if (!(widthPx > 0 && heightPx > 0)) return { columns: 0, rows: 0 };
	const middle = (box.south + box.north) / 2;
	const metresPerPx = (EARTH_CIRCUMFERENCE_M * Math.cos((middle * Math.PI) / 180)) / world;
	const cellPx = Math.max(
		MIN_CELL_PX,
		MIN_CELL_M / metresPerPx,
		Math.sqrt((widthPx * heightPx) / MAX_CELLS)
	);
	return {
		columns: Math.max(1, Math.floor(widthPx / cellPx)),
		rows: Math.max(1, Math.floor(heightPx / cellPx))
	};
}

/** The five classes, as the key says them, the gentlest first. */
export const SLOPE_KEY: readonly { words: string; color: string }[] = [
	'Under 25°',
	'25–30°',
	'30–35°',
	'35–40°',
	'40° and over'
].map((words, index) => ({ words, color: SLOPE_ON_MAP[index] }));

/** One colour as its red, green and blue. */
function rgb(hex: string): [number, number, number] {
	const value = Number.parseInt(hex.slice(1, 7), 16);
	return [(value >> 16) & 255, (value >> 8) & 255, value & 255];
}

const COLOURS = SLOPE_ON_MAP.map(rgb);

/**
 * The picture's pixels, one a cell, row after row from the north-west: a
 * class in its colour, nothing where the ground is not loaded (0).
 */
export function slopePixels(classes: Uint8Array): Uint8ClampedArray<ArrayBuffer> {
	const pixels = new Uint8ClampedArray(classes.length * 4);
	classes.forEach((one, index) => {
		const colour = COLOURS[one - 1];
		if (!colour) return;
		pixels.set([...colour, 255], index * 4);
	});
	return pixels;
}

/** How many cells of each class, the gentlest first, and how many have one. */
export function slopeCounts(classes: Uint8Array): { classed: number; byClass: number[] } {
	const byClass = [0, 0, 0, 0, 0];
	for (const one of classes) if (one > 0) byClass[one - 1] += 1;
	return { classed: byClass.reduce((sum, count) => sum + count, 0), byClass };
}

/** The four corners of a box as an image source of the map takes them. */
export function cornersOf(
	box: Bounds
): [[number, number], [number, number], [number, number], [number, number]] {
	return [
		[box.west, box.north],
		[box.east, box.north],
		[box.east, box.south],
		[box.west, box.south]
	];
}
