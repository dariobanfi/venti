/**
 * How steep the ground is over the part of the map in view, as the worker
 * finds it out for the map of slopes: the core reads each place's
 * steepness and class (`slopeClasses`) over the elevation tiles this file
 * fetches for the box, the same tiles the glide's reach and the ground
 * under a flight are read from.
 *
 * The tiles are kept between questions, so that a map moved a little
 * fetches only those at its new edge; past `TILES_AT_MOST` they are let
 * go of and the box's own fetched anew, a tile's heights being half a
 * megabyte. Only the newest question is worked on.
 */

import { ONLINE } from '../map/online';
import { type Bounds, type Coordinate, slopeClasses, Terrain } from './pkg/venti_core_wasm.js';
import { fetchTiles, type GetTile, getTile } from './terrain-source';

/** How many tiles are kept before they are let go of: some 25 MB of heights. */
export const TILES_AT_MOST = 48;

/** The map of slopes asked for: a grid of cells over a box. */
export interface SlopesAsked {
	/**
	 * A number the page gives the question, to let go of it by. Only the
	 * newest question is worked on.
	 */
	asking: number;
	box: Bounds;
	/** Cells from west to east, and from north to south. */
	columns: number;
	rows: number;
}

/** The class of each cell. */
export interface SlopesAnswer {
	asking: number;
	box: Bounds;
	columns: number;
	rows: number;
	/**
	 * Row after row from the north-west, one per cell: 0 where the ground
	 * is not loaded, else the core's class counted from 1 (`under25`) to 5
	 * (`from40`).
	 */
	classes: Uint8Array;
}

/** The core's classes, in the order the answer counts them from 1. */
export const SLOPE_BANDS = ['under25', 'from25', 'from30', 'from35', 'from40'] as const;

/** The middle of each cell of the grid, row after row from the north-west. */
export function cellMiddles(box: Bounds, columns: number, rows: number): Coordinate[] {
	const height = box.north - box.south;
	const width = box.east - box.west;
	const middles: Coordinate[] = [];
	for (let row = 0; row < rows; row += 1) {
		const lat = box.north - (height * (row + 0.5)) / rows;
		for (let column = 0; column < columns; column += 1) {
			middles.push({ lat, lon: box.west + (width * (column + 0.5)) / columns });
		}
	}
	return middles;
}

export class SlopesSource {
	#get: GetTile;
	#tiles: string;
	#terrain: Terrain | undefined;
	/** How many tiles were handed to the terrain kept. */
	#held = 0;
	#asking: number | undefined;

	constructor(get: GetTile = getTile, tiles: string = ONLINE.terrainTiles) {
		this.#get = get;
		this.#tiles = tiles;
	}

	/** The classes over the box asked for; nothing for a question asked since. Never throws for a tile. */
	async classes(asked: SlopesAsked): Promise<SlopesAnswer | undefined> {
		this.#asking = asked.asking;
		const wanted = () => this.#asking === asked.asking;
		const { south, west, north, east } = asked.box;
		let terrain = (this.#terrain ??= new Terrain());
		let coming = terrain.tilesCovering(south, west, north, east).length;
		if (this.#held + coming > TILES_AT_MOST) {
			this.forget();
			terrain = this.#terrain = new Terrain();
			coming = terrain.tilesCovering(south, west, north, east).length;
		}
		this.#held += coming;
		await fetchTiles(terrain, asked.box, this.#get, this.#tiles, wanted);
		if (!wanted() || this.#terrain !== terrain) return undefined;
		const found = slopeClasses(cellMiddles(asked.box, asked.columns, asked.rows), terrain);
		const classes = new Uint8Array(found.length);
		found.forEach((one, index) => {
			classes[index] = one.class ? SLOPE_BANDS.indexOf(one.class) + 1 : 0;
		});
		return { ...asked, classes };
	}

	/** Lets go of the tiles kept: the layer was switched off. */
	forget(): void {
		this.#terrain?.free();
		this.#terrain = undefined;
		this.#held = 0;
	}
}
