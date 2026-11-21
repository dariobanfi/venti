/**
 * What a glide reaches from a place on the map, as the worker finds it
 * out: the core lays the glide over the elevation tiles this file fetches
 * around the place (`glideReach`).
 *
 * The tiles are kept for the place, so that a change of the glide, the
 * start height or the wind asks the core again and fetches nothing. Where
 * the core says the glide ran off the tiles (`cutShort`), the tiles of a
 * wider box are fetched and the core asked again, up to the widest box
 * below. Another place, or "Clear", lets go of them: a tile's heights are
 * half a megabyte.
 */

import { ONLINE } from '../map/online';
import {
	type Bounds,
	type GlideReach,
	glideReach,
	type GlideReachSetting,
	Terrain
} from './pkg/venti_core_wasm.js';
import { fetchTiles, type GetTile, getTile } from './terrain-source';

/**
 * How far from the place the tiles reach, kilometres each way: first
 * enough for a summit start at a glide of 8 in the Alps, then wider while
 * the glide runs off them. A start of 3000 m over the valleys at a glide
 * of 10 stays within the widest.
 */
export const REACH_BOXES_KM: readonly number[] = [10, 25, 50];

const KM_PER_DEGREE = 111.32;

/** The box reaching `km` each way from a place. */
export function boxAround(lat: number, lon: number, km: number): Bounds {
	const dLat = km / KM_PER_DEGREE;
	const dLon = km / (KM_PER_DEGREE * Math.max(Math.cos((lat * Math.PI) / 180), 0.01));
	return { south: lat - dLat, west: lon - dLon, north: lat + dLat, east: lon + dLon };
}

interface Kept {
	/** The place the tiles are around. */
	key: string;
	terrain: Terrain;
	/** The widest of `REACH_BOXES_KM` filled so far, by its index; -1 for none. */
	filled: number;
	/** The fetching under way, one box after another. */
	filling: Promise<void>;
}

export class GlideReachSource {
	#get: GetTile;
	#tiles: string;
	#kept: Kept | undefined;

	constructor(get: GetTile = getTile, tiles: string = ONLINE.terrainTiles) {
		this.#get = get;
		this.#tiles = tiles;
	}

	/**
	 * What a glide reaches from the setting's place, over the tiles around
	 * it. For a place asked about since, the answer over no ground (an
	 * empty outline), which the page drops. Never throws for a tile.
	 */
	async reach(setting: GlideReachSetting): Promise<GlideReach> {
		const key = `${setting.lat},${setting.lon}`;
		if (this.#kept?.key !== key) {
			this.forget();
			this.#kept = { key, terrain: new Terrain(), filled: -1, filling: Promise.resolve() };
		}
		const kept = this.#kept;
		for (let step = Math.max(kept.filled, 0); ; step += 1) {
			await this.#fill(kept, setting, step);
			if (this.#kept !== kept) return this.#overNothing(setting);
			const reach = glideReach(setting, kept.terrain);
			if (!reach.cutShort || step >= REACH_BOXES_KM.length - 1) return reach;
		}
	}

	/** Lets go of the tiles kept: the area was cleared. */
	forget(): void {
		this.#kept?.terrain.free();
		this.#kept = undefined;
	}

	/** Fetches the tiles of the box `step`, after those asked for before; once. */
	#fill(kept: Kept, setting: GlideReachSetting, step: number): Promise<void> {
		if (kept.filled >= step) return kept.filling;
		kept.filling = kept.filling.then(async () => {
			if (kept.filled >= step || this.#kept !== kept) return;
			const box = boxAround(setting.lat, setting.lon, REACH_BOXES_KM[step]);
			await fetchTiles(kept.terrain, box, this.#get, this.#tiles, () => this.#kept === kept);
			if (this.#kept === kept) kept.filled = Math.max(kept.filled, step);
		});
		return kept.filling;
	}

	#overNothing(setting: GlideReachSetting): GlideReach {
		const nothing = new Terrain();
		try {
			return glideReach(setting, nothing);
		} finally {
			nothing.free();
		}
	}
}
