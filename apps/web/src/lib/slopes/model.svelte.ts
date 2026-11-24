/**
 * The map of slopes on the map tab: when to ask the core's worker for the
 * classes of the part of the map in view, and the picture the map draws of
 * them. Asked once the map has rested a moment close enough
 * (`SLOPES_MIN_ZOOM`), never while it moves; the picture before stays
 * until the next one is there.
 */

import { messageOf } from '#lib/core/index.ts';
import type { Bounds, SlopesAnswer, SlopesAsked } from '#lib/core/index.ts';
import { cornersOf, gridFor, slopeCounts, slopePixels, SLOPES_MIN_ZOOM } from './grid';

/** How long the map rests before the slopes are asked for, in milliseconds. */
export const REST_MS = 350;

/** What the map draws of the slopes. */
export interface SlopesPicture {
	/** The picture, one pixel a cell, as an address the map can load. */
	url: string;
	corners: ReturnType<typeof cornersOf>;
	/** How many cells the grid has, and how many of them have a class, by class. */
	cells: number;
	classed: number;
	byClass: number[];
}

export type AskSlopes = (asked: SlopesAsked) => Promise<SlopesAnswer | undefined>;

/** Makes the picture's address from its pixels. */
export type Paint = (
	pixels: Uint8ClampedArray<ArrayBuffer>,
	columns: number,
	rows: number
) => string;

/** Paints the pixels on a canvas of the page and hands them over as a PNG. */
export const paintOnCanvas: Paint = (pixels, columns, rows) => {
	const canvas = document.createElement('canvas');
	canvas.width = columns;
	canvas.height = rows;
	canvas.getContext('2d')?.putImageData(new ImageData(pixels, columns, rows), 0, 0);
	return canvas.toDataURL('image/png');
};

export class SlopesModel {
	/** The picture of the last answer. */
	picture = $state.raw<SlopesPicture>();
	/** Whether the map is close enough to draw them. */
	close = $state(false);
	/** Why the slopes could not be had, in words for the pilot. */
	problem = $state.raw<string>();

	#ask: AskSlopes;
	#forget: () => void;
	#paint: Paint;
	#asking = 0;
	#resting: ReturnType<typeof setTimeout> | undefined;

	constructor(ask: AskSlopes, forget: () => void, paint: Paint = paintOnCanvas) {
		this.#ask = ask;
		this.#forget = forget;
		this.#paint = paint;
	}

	/** The picture the map draws: none while it is zoomed out. */
	get shown(): SlopesPicture | undefined {
		return this.close ? this.picture : undefined;
	}

	/** The part of the map in view has come to rest: asks for its slopes once it has rested a moment. */
	show(region: Bounds, zoom: number): void {
		this.close = zoom >= SLOPES_MIN_ZOOM;
		clearTimeout(this.#resting);
		if (!this.close) return;
		this.#resting = setTimeout(() => void this.#load(region, zoom), REST_MS);
	}

	/** The layer was switched off: lets go of the picture and of the ground the worker keeps. */
	off(): void {
		clearTimeout(this.#resting);
		this.#asking += 1;
		this.picture = undefined;
		this.problem = undefined;
		this.#forget();
	}

	async #load(region: Bounds, zoom: number): Promise<void> {
		const { columns, rows } = gridFor(region, zoom);
		if (columns * rows === 0) return;
		const asking = ++this.#asking;
		try {
			const answer = await this.#ask({ asking, box: region, columns, rows });
			if (!answer || asking !== this.#asking) return;
			const counts = slopeCounts(answer.classes);
			this.picture = {
				url: this.#paint(slopePixels(answer.classes), answer.columns, answer.rows),
				corners: cornersOf(answer.box),
				cells: answer.classes.length,
				...counts
			};
			this.problem = undefined;
		} catch (error) {
			if (asking !== this.#asking) return;
			this.problem = messageOf(error);
		}
	}
}
