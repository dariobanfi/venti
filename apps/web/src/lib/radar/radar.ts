/**
 * The "Radar" layer of the map tab: the last frames of rain a weather
 * service's radars saw, and where there is one an analysis of the rain
 * beside them, as pictures over the map, a layer for each source.
 *
 * Everything the layer says is the core's (`radar`): the moments of the
 * strip and which frame of which layer lies on the map at each, every
 * time and age, when the newest is too old to be the rain of now, the key,
 * the note and the credits. This file only says which moment is looked
 * at, what the strip writes in its cells, which colours of the design
 * pack the core's key names, when the server is asked again, and holds
 * the few words of the app's own, for when there is nothing to read.
 */

import type {
	RadarColours,
	RadarKey,
	RadarLayerView,
	RadarMoment,
	RadarView
} from '#lib/core/index.ts';
import { keyColor, NOT_FLYABLE_ON_MAP } from '#lib/design/palette.ts';

/** How long a list of frames is shown before the server is asked again: a frame comes every five minutes. */
export const ASK_EVERY_MS = 5 * 60_000;
/** How often the frames' ages are said again, and whether the list is due is looked at. */
export const AGES_EVERY_MS = 60_000;
/** How long a moment stands while the moments are played: four a second. */
export const PLAY_MS = 250;

/** From how many moments on the strip writes the minutes in every third cell only. */
const ALL_LABELLED_UP_TO = 8;

/** The app's own words, where the core has none. */
export const WORDS = {
	asking: 'Asking Venti for the rain radar.',
	askingPicture: 'Asking Venti for the radar picture.',
	unreachable: 'Venti could not be reached for the rain radar.',
	noServer: 'This copy of the app is served without Venti behind it, so there is no rain radar.'
} as const;

/**
 * The moment to show: the one of the time the pilot chose, while the list
 * still has it, and otherwise the one the core opens on, the newest.
 */
export function momentOf(
	view: RadarView | undefined,
	chosenTimeUnixS: number | undefined
): RadarMoment | undefined {
	return (
		view?.moments.find((moment) => moment.timeUnixS === chosenTimeUnixS) ??
		view?.moments[view.firstMoment]
	);
}

/**
 * What choosing a cell of the strip is remembered as: the moment's time,
 * and nothing for the one the core opens on, so that the layer goes on to
 * the next moment when one arrives only if the pilot was looking at the
 * newest.
 */
export function chosenTime(view: RadarView, index: number): number | undefined {
	return index === view.firstMoment ? undefined : view.moments[index]?.timeUnixS;
}

/** One layer's picture of a moment, as the map draws it. */
export interface Drawn {
	/** Which layer: its place in the view's layers, the lowest first. */
	layer: number;
	/** The path of the frame the picture is of. */
	path: string;
	/**
	 * The paths of the frames drawn over it at the moment, the lowest
	 * first: the picture is clear wherever one of them sees the place.
	 */
	above: string[];
	/** Whether what the source did not see is drawn as the veil; clear in a picture that lies over another. */
	drawsUnseen: boolean;
	/** The corners the layer's pictures are pinned by. */
	corners: Corners;
	/** Whether the layer's newest frame is too old to be the rain of now: its picture is drawn fainter. */
	faded: boolean;
}

/**
 * What a moment draws, the lowest first, as the core says: for each of its
 * frames the layer's picture at that frame's path, with the veil of "not
 * seen" only where the moment says so, and the frames that lie over it,
 * under which the core makes it clear (`radarFrameImageUnder`). A layer
 * without a frame at the moment is not among them.
 */
export function drawn(view: RadarView, moment: RadarMoment | undefined): Drawn[] {
	const pictures = (moment?.frames ?? []).flatMap((at) => {
		const layer = view.layers[at.layer];
		const frame = layer?.frames[at.frame];
		return layer && frame
			? [
					{
						layer: at.layer,
						path: frame.path,
						drawsUnseen: at.drawsUnseen,
						corners: corners(layer),
						faded: layer.stale
					}
				]
			: [];
	});
	return pictures.map((picture, place) => ({
		...picture,
		above: pictures.slice(place + 1).map((over) => over.path)
	}));
}

/** What says which picture one is: its frame, the veil, and the frames over it. */
export type Pictured = Pick<Drawn, 'path' | 'drawsUnseen' | 'above'>;

/**
 * What a picture is kept by for the visit: its frame's path, which names
 * its layer, whether the veil is drawn in it, and the paths of the frames
 * that lie over it. The same frame of the analysis lies under another
 * frame of the radars at every moment, and is another picture under each.
 */
export function pictureKey(picture: Pictured): string {
	const under = picture.above.map((path) => ` under ${path}`).join('');
	return `${picture.path}${picture.drawsUnseen ? ' veiled' : ''}${under}`;
}

/** The pictures of a moment in one word: two moments that draw the same have the same. */
export function drawnKey(pictures: readonly Pictured[]): string {
	return pictures.map(pictureKey).join('\n');
}

/**
 * The layer whose caution the card says, when it is behind: the topmost
 * that has frames, and the topmost while none has, which then says that
 * there is no picture yet.
 */
export function cautioned(view: RadarView | undefined): RadarLayerView | undefined {
	return view?.layers.findLast((layer) => layer.frames.length > 0) ?? view?.layers.at(-1);
}

/**
 * The cells of the strip, the oldest first, a cell a moment: the minutes
 * of the moment's time on the clock, "15", in every cell of a short strip
 * and in every third of a long one, counted from the newest; and for a
 * screen reader the core's line for the moment, "Rain radar, 14:15, 4
 * minutes ago. Austria's analysis is of 14:00."
 */
export function stripCells(moments: readonly RadarMoment[]): { short: string; label: string }[] {
	const last = moments.length - 1;
	return moments.map((moment, index) => ({
		short:
			moments.length <= ALL_LABELLED_UP_TO || (last - index) % 3 === 0
				? moment.clock.slice(-2)
				: '',
		label: moment.label
	}));
}

/** A colour of the pack, "#RRGGBB", as the core takes it: `0xRRGGBBAA`, solid. */
export function solid(colour: string): number {
	return Number.parseInt(colour.slice(1, 7), 16) * 0x100 + 0xff;
}

/** No colour at all: what a picture did not see, where the picture lies over another. */
export const CLEAR = 0;

/**
 * The colours a picture is written in: for each step of rain the colour its
 * line of the key names, the first step first, and for what the source did
 * not see the forecast map's veil where the picture draws it, and nothing
 * where it lies over another, whose rain the veil would hide. The colours
 * are solid: how much of the map shows through is the layer's
 * (`radar-layer.ts`).
 */
export function colours(key: readonly RadarKey[], drawsUnseen: boolean): RadarColours {
	return {
		rain: key.toSorted((one, other) => one.step - other.step).map((line) => solid(keyColor(line))),
		unseen: drawsUnseen ? solid(UNSEEN_ON_MAP) : CLEAR
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
