/**
 * The cables of the map tab: when to ask the server for them and what the
 * map draws. They are asked for around the map's middle once the map has
 * come to rest close enough to see a valley (`CABLES_MIN_ZOOM`), and asked
 * again when the middle has moved far enough that the box asked for no
 * longer lies well around it. What the core answered is kept while the
 * map is zoomed out, and drawn again when it comes back.
 */

import type { Bounds, CableLine, CablesMap, Coordinate } from '#lib/core/index.ts';

/** From this zoom on the map draws cables, and asks for them. */
export const CABLES_MIN_ZOOM = 9;

/**
 * How far the middle may move from the place asked about before the
 * cables are asked for again, in degrees: half of the box the core asks
 * for (`cablesRequest`, 0.3 by 0.45 each way).
 */
const MOVE_LAT_DEG = 0.15;
const MOVE_LON_DEG = 0.225;

export type AskCables = (place: Coordinate) => Promise<CablesMap | undefined>;

export class CablesModel {
	/** What the core made of the server's last answer. */
	answer = $state.raw<CablesMap>();
	/** Whether the map is close enough to draw them. */
	close = $state(false);
	/** Why the cables could not be had, in words for the pilot. */
	problem = $state.raw<string>();

	#ask: AskCables;
	#asked: Coordinate | undefined;
	#asking = 0;

	constructor(ask: AskCables) {
		this.#ask = ask;
	}

	/** The cables the map draws: none while it is zoomed out. */
	get lines(): readonly CableLine[] {
		return this.close ? (this.answer?.lines ?? []) : [];
	}

	/** One cable the map draws, by its identifier. */
	line(id: string): CableLine | undefined {
		return this.lines.find((line) => line.id === id);
	}

	/** The part of the map in view has come to rest: asks for its cables where it is time. */
	show(region: Bounds, zoom: number): void {
		this.close = zoom >= CABLES_MIN_ZOOM;
		if (!this.close) return;
		const middle = { lat: (region.south + region.north) / 2, lon: (region.west + region.east) / 2 };
		const asked = this.#asked;
		if (
			asked &&
			Math.abs(asked.lat - middle.lat) <= MOVE_LAT_DEG &&
			Math.abs(asked.lon - middle.lon) <= MOVE_LON_DEG
		) {
			return;
		}
		this.#asked = middle;
		const mine = ++this.#asking;
		this.#ask(middle).then(
			(answer) => {
				if (mine !== this.#asking) return;
				this.answer = answer;
				this.problem = undefined;
			},
			(error: unknown) => {
				if (mine !== this.#asking) return;
				// Asked again at the next rest of the map.
				this.#asked = undefined;
				this.problem = error instanceof Error ? error.message : String(error);
			}
		);
	}
}
