/**
 * The ground under a flight, as the worker finds it out: for the filled
 * shape along the foot of a flight's altitude profile.
 *
 * The heights come from the elevation tiles the map shades its relief
 * from. The core says which tiles cover a flight and reads the heights out
 * of them; this file fetches the tiles in between, from the tile server
 * the map's style names and from no other, as the map itself would. It is
 * the browser's counterpart of `ground(under:)` in the iPhone app's
 * `TerrainModel.swift`.
 *
 * Nothing is kept. The browser's own cache may hold a tile, for as long
 * as the tile server allows, which is also how the map and this file share
 * what either has fetched. The heights are let go of as soon as the ground
 * under the flight is read: a tile's are half a megabyte.
 */

import { ONLINE } from '../map/online';
import { type GroundUnderFlight, Terrain, type TrackPoint } from './pkg/venti_core_wasm.js';
import type { Bounds } from './pkg/venti_core_wasm.js';

/**
 * At how many moments of a flight the ground is read: more than the chart
 * is points wide. The iPhone reads as many.
 */
export const PLACES_UNDER_A_FLIGHT = 240;

/** How many tiles are on their way at a time. */
const AT_ONCE = 6;

/** How long a tile may take, in milliseconds. */
const PATIENCE_MS = 20_000;

/** Fetches one tile by its address. Throws when no answer came at all. */
export type GetTile = (address: string) => Promise<{ status: number; body: Uint8Array }>;

/** Asks as the map asks, so that the browser's cache serves them both. */
const getTile: GetTile = async (address) => {
	const response = await fetch(address, { signal: AbortSignal.timeout(PATIENCE_MS) });
	return { status: response.status, body: new Uint8Array(await response.arrayBuffer()) };
};

/** A flight whose ground is asked for. */
export interface GroundAsked {
	/**
	 * A number the page gives the question, to let go of it by. Only the
	 * newest question is worked on.
	 */
	asking: number;
	/** The box the flight lies in. */
	bounds: Bounds;
	track: TrackPoint[];
}

export class TerrainSource {
	#get: GetTile;
	/** The tile server's address, with `{z}/{x}/{y}` for the tile. */
	#tiles: string;
	/** The question being worked on, if one is. */
	#asking: number | undefined;

	constructor(get: GetTile = getTile, tiles: string = ONLINE.terrainTiles) {
		this.#get = get;
		this.#tiles = tiles;
	}

	/**
	 * The ground under a flight's track, at up to 240 moments of it, in
	 * order. Empty where there is no ground to be had: no tile could be
	 * fetched, the tile server has none for the place, or the page has
	 * asked about another flight or let go of this one by now. A moment
	 * whose tile is missing is left out. Never throws for a tile.
	 */
	async under({ asking, bounds, track }: GroundAsked): Promise<GroundUnderFlight[]> {
		this.#asking = asking;
		const wanted = () => this.#asking === asking;
		const terrain = new Terrain();
		try {
			const tiles = terrain.tilesCovering(bounds.south, bounds.west, bounds.north, bounds.east);
			let reachable = true;
			const bring = async () => {
				for (;;) {
					const tile = tiles.shift();
					// Without an answer to one tile the others would not come
					// either: the browser is offline, or the tile server is.
					if (!tile || !reachable || !wanted()) return;
					try {
						const answer = await this.#get(this.#tiles.replace('{z}/{x}/{y}', tile.path));
						// The tile server has no tile here: that ground stays unknown.
						if (answer.status !== 200) continue;
						terrain.addTile(tile.z, tile.x, tile.y, answer.body);
					} catch (error) {
						// Bytes that are no elevation tile leave a gap and no more.
						if (!(error instanceof Error) || error.name !== 'InvalidFile') reachable = false;
					}
				}
			};
			await Promise.all(Array.from({ length: AT_ONCE }, bring));
			return wanted() ? terrain.groundUnder(track, PLACES_UNDER_A_FLIGHT) : [];
		} finally {
			terrain.free();
			if (wanted()) this.#asking = undefined;
		}
	}

	/** Lets go of a question: the tiles not asked for yet are not fetched. */
	forget(asking: number): void {
		if (this.#asking === asking) this.#asking = undefined;
	}
}
