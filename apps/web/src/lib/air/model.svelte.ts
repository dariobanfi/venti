/**
 * The two layers of the map tab that say what many gliders say together,
 * both off until the pilot switches them on: the climbs of the last
 * minutes ("Climbs now"), asked for the part of the map in view and again
 * every half minute while on, and where thermals have been ("Where
 * thermals have been"), asked for the part in view, in every month.
 * Every word is the core's.
 */

import type {
	Bounds,
	ClimbView,
	FlownAnswer,
	FlownCellView,
	ClimbsAnswer
} from '#lib/core/index.ts';
import { airKey } from './on-map';

/** How often the climbs are asked for again while their layer is on. */
export const CLIMBS_EVERY_MS = 30_000;

const said = (error: unknown) => (error instanceof Error ? error.message : String(error));

export class ClimbsModel {
	/** What the core made of the server's last answer. */
	answer = $state.raw<ClimbsAnswer>();
	/** Why the climbs could not be had, in words for the pilot. */
	problem = $state.raw<string>();

	#ask: (box: Bounds) => Promise<ClimbsAnswer | undefined>;
	#region: Bounds | undefined;
	#asking = 0;

	constructor(ask: (box: Bounds) => Promise<ClimbsAnswer | undefined>) {
		this.#ask = ask;
	}

	get climbs(): readonly ClimbView[] {
		return this.answer?.climbs ?? [];
	}

	/** One climb on the map, by its key. */
	climb(key: string): ClimbView | undefined {
		return this.climbs.find((climb) => airKey(climb) === key);
	}

	/** The part of the map in view has come to rest: its climbs. */
	show(region: Bounds): void {
		this.#region = region;
		this.#again();
	}

	/** Asks again every half minute until the returned function is called. */
	keepCurrent(): () => void {
		const timer = setInterval(() => this.#again(), CLIMBS_EVERY_MS);
		return () => clearInterval(timer);
	}

	/** The layer was switched off: nothing of it stays. */
	off(): void {
		this.#asking += 1;
		this.answer = undefined;
		this.problem = undefined;
	}

	#again(): void {
		const region = this.#region;
		if (!region) return;
		const mine = ++this.#asking;
		this.#ask(region).then(
			(answer) => {
				if (mine !== this.#asking) return;
				this.answer = answer;
				this.problem = undefined;
			},
			(error: unknown) => {
				if (mine !== this.#asking) return;
				this.problem = said(error);
			}
		);
	}
}

export class FlownModel {
	/** The core's answer for the part of the map asked about last. */
	answer = $state.raw<FlownAnswer>();
	/** Why the thermals could not be had, in words for the pilot. */
	problem = $state.raw<string>();

	#ask: (box: Bounds) => Promise<FlownAnswer | undefined>;
	#asking = 0;

	constructor(ask: (box: Bounds) => Promise<FlownAnswer | undefined>) {
		this.#ask = ask;
	}

	get cells(): readonly FlownCellView[] {
		return this.answer?.thermals?.cells ?? [];
	}

	/** One cell on the map, by its key. */
	cell(key: string): FlownCellView | undefined {
		return this.cells.find((cell) => airKey(cell) === key);
	}

	/** The part of the map in view has come to rest: its thermals. */
	show(region: Bounds): void {
		const mine = ++this.#asking;
		this.#ask(region).then(
			(answer) => {
				if (mine !== this.#asking) return;
				this.answer = answer;
				this.problem = undefined;
			},
			(error: unknown) => {
				if (mine !== this.#asking) return;
				this.problem = said(error);
			}
		);
	}

	/** The layer was switched off: nothing of it stays. */
	off(): void {
		this.#asking += 1;
		this.answer = undefined;
		this.problem = undefined;
	}
}
