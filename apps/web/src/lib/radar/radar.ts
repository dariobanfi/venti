/**
 * The "Radar" layer of the map tab: the last frames of rain a weather
 * service's radars saw, as pictures over the map.
 *
 * Everything the layer says is the core's (`radar`): each frame's time and
 * age, when the newest is too old to be the rain of now, the key, the note
 * and the credit. This file only says which frame is shown, what the strip
 * of frames writes in its cells, which colours of the design pack the
 * core's key names, when the server is asked again, and holds the few
 * words of the app's own, for when there is nothing to read.
 */

import type {
	RadarColours,
	RadarFrameView,
	RadarKey,
	RadarLayerView,
	RadarView
} from '#lib/core/index.ts';
import { keyColor, NOT_FLYABLE_ON_MAP } from '#lib/design/palette.ts';

/** How long a list of frames is shown before the server is asked again: a frame comes every five minutes. */
export const ASK_EVERY_MS = 5 * 60_000;
/** How often the frames' ages are said again, and whether the list is due is looked at. */
export const AGES_EVERY_MS = 60_000;
/** How long a frame stands while the frames are played: four a second. */
export const PLAY_MS = 250;

/** From how many frames on the strip writes the minutes in every third cell only. */
const ALL_LABELLED_UP_TO = 8;

/** The app's own words, where the core has none. */
export const WORDS = {
	asking: 'Asking Venti for the rain radar.',
	askingPicture: 'Asking Venti for the radar picture.',
	unreachable: 'Venti could not be reached for the rain radar.',
	noServer: 'This copy of the app is served without Venti behind it, so there is no rain radar.'
} as const;

/** The layer that is shown: the first, of the one radar source a server has today. */
export function layerOf(view: RadarView | undefined): RadarLayerView | undefined {
	return view?.layers[0];
}

/**
 * The frame to show: the one of the time the pilot chose, while the layer
 * still has it, and the newest otherwise.
 */
export function frameOf(
	layer: RadarLayerView | undefined,
	chosenTimeUnixS: number | undefined
): RadarFrameView | undefined {
	return layer?.frames.find((frame) => frame.timeUnixS === chosenTimeUnixS) ?? layer?.frames.at(-1);
}

/**
 * What choosing a cell of the strip is remembered as: the frame's time, and
 * nothing for the newest, so that the layer goes on to the next frame when
 * one arrives only if the pilot was looking at the newest.
 */
export function chosenTime(layer: RadarLayerView, index: number): number | undefined {
	return index >= layer.frames.length - 1 ? undefined : layer.frames[index]?.timeUnixS;
}

/**
 * The cells of the strip of frames, the oldest first: the minutes of the
 * frame's time on the clock, "35", in every cell of a short strip and in
 * every third of a long one, counted from the newest; and for a screen
 * reader the time and the age, "11:35, 9 minutes ago".
 */
export function stripCells(frames: readonly RadarFrameView[]): { short: string; label: string }[] {
	const last = frames.length - 1;
	return frames.map((frame, index) => ({
		short:
			frames.length <= ALL_LABELLED_UP_TO || (last - index) % 3 === 0 ? frame.clock.slice(-2) : '',
		label: `${frame.clock}, ${frame.age}`
	}));
}

/** A colour of the pack, "#RRGGBB", as the core takes it: `0xRRGGBBAA`, solid. */
export function solid(colour: string): number {
	return Number.parseInt(colour.slice(1, 7), 16) * 0x100 + 0xff;
}

/**
 * The colours a picture is written in: for each step of rain the colour its
 * line of the key names, the first step first, and the forecast map's veil
 * for what the radar did not see. All solid: how much of the map shows
 * through is the layer's (`radar-layer.ts`).
 */
export function colours(key: readonly RadarKey[]): RadarColours {
	return {
		rain: key.toSorted((one, other) => one.step - other.step).map((line) => solid(keyColor(line))),
		unseen: solid(UNSEEN_ON_MAP)
	};
}

/** What the radar did not see: the quiet veil the forecast map lays over a day that is not flyable. */
export const UNSEEN_ON_MAP = NOT_FLYABLE_ON_MAP;

/** The corners a picture is pinned to the map by: top left, top right, bottom right, bottom left. */
export type Corners = [[number, number], [number, number], [number, number], [number, number]];

export function corners(layer: Pick<RadarLayerView, 'west' | 'south' | 'east' | 'north'>): Corners {
	return [
		[layer.west, layer.north],
		[layer.east, layer.north],
		[layer.east, layer.south],
		[layer.west, layer.south]
	];
}

/**
 * A picture as an address the map can load: the PNG's bytes written into a
 * `data:` address, which reaches no host and which the page's content
 * security policy already lets the map fetch.
 */
export function pictureAddress(png: Uint8Array): string {
	let text = '';
	// In pieces: a call takes only so many arguments.
	for (let from = 0; from < png.length; from += 0x8000) {
		text += String.fromCharCode(...png.subarray(from, from + 0x8000));
	}
	return `data:image/png;base64,${btoa(text)}`;
}

/** Whether the server is to be asked for the list: never asked, or five minutes ago. */
export function isDue(askedMs: number | undefined, nowMs: number): boolean {
	return askedMs === undefined || nowMs - askedMs >= ASK_EVERY_MS;
}
