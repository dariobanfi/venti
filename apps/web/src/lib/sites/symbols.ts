/**
 * The pins the map draws flying sites with, as the design pack draws them:
 * the pack's indigo and cream, with the tip on the site, so that a site is
 * never taken for wind, airspace or a track, which have the colours. In the
 * pin's head a take-off has the pack's take-off icon and a landing its
 * landing icon (the owner, 2026-11-22: "It needs to be clearer what's the
 * landing … with a different icon"), read from the pack's own files.
 *
 * `SiteGlyph.svelte` draws the same pin in a list, so the map and the list
 * show one sign.
 */

import type { SiteKind } from '#lib/core/index.ts';
import {
	INK_ON_MAP,
	MUTED_INK_ON_MAP,
	MUTED_PAPER_ON_MAP,
	PAPER_ON_MAP
} from '#lib/design/palette.ts';
import type { Picture } from '#lib/wind/symbols.ts';
import takeoffFile from '../../../../../design/assets/icons/takeoff.svg?raw';
import landingFile from '../../../../../design/assets/icons/landing.svg?raw';

/** The pin's box, in points: a little narrower than tall. */
export const PIN_WIDTH = 24;
export const PIN_HEIGHT = 32;

/**
 * The design pack's pin, with its tip at the bottom middle of the box, on
 * the site: a round head over a point.
 */
export const PIN = 'M12 31C12 31 2.5 20 2.5 11.5a9.5 9.5 0 0 1 19 0C21.5 20 12 31 12 31Z';
/** Where the mark in the pin's head sits, and how large it is. */
export const MARK = { x: 12, y: 11.5, radius: 4 } as const;
/**
 * How large the pack's 24-point icon is drawn in the pin's head: within
 * its edge, its 2-point stroke a little over one point.
 */
export const ICON_SCALE = 0.6;
/** Where the icon's box begins, so that its middle is the head's. */
export const ICON_AT = { x: MARK.x - 12 * ICON_SCALE, y: MARK.y - 12 * ICON_SCALE } as const;
/** The width of the pin's edge, and of a ring. */
export const EDGE_WIDTH = 1.5;
export const RING_WIDTH = 2.2;

export const SITE_KINDS: readonly SiteKind[] = [
	'takeoff',
	'landing',
	'towing',
	'training',
	'other'
];

/**
 * The name of a kind's picture. `apart` is the pin of a take-off whose
 * source does not say what the filter asks: the same pin, muted.
 */
export function pictureName(kind: SiteKind, apart = false): string {
	return apart ? `site-${kind}-apart` : `site-${kind}`;
}

/**
 * How a kind of site is drawn, in the pack's indigo and cream: a place to
 * take off from is a dark pin with a cream dot, a landing the same turned
 * round, cream with a dark dot; a training slope or a site of another kind
 * has a ring in place of the dot.
 */
export function colours(kind: SiteKind, apart = false): { ground: string; sign: string } {
	// Apart, the pack's muted pair stands in for the ink and the paper: the
	// pin keeps its shape and says less.
	const ink = apart ? MUTED_INK_ON_MAP : INK_ON_MAP;
	const paper = apart ? MUTED_PAPER_ON_MAP : PAPER_ON_MAP;
	return kind === 'landing' ? { ground: paper, sign: ink } : { ground: ink, sign: paper };
}

/** Whether the pin's head holds a ring rather than an icon. */
export function ringed(kind: SiteKind): boolean {
	return kind === 'training' || kind === 'other';
}

/** One shape of a pack's icon: a path, or an ellipse or circle. */
export type Shape =
	{ kind: 'path'; d: string } | { kind: 'ellipse'; cx: number; cy: number; rx: number; ry: number };

/** The shapes inside the `svg` element of one of the pack's icons, as its file has them. */
export function shapesOf(file: string): Shape[] {
	const number = (tag: string, name: string) =>
		Number(new RegExp(`\\s${name}="([^"]*)"`).exec(tag)?.[1] ?? 0);
	return [...file.matchAll(/<(path|ellipse|circle)\b[^>]*>/g)].map(([tag, name]) =>
		name === 'path'
			? { kind: 'path', d: /\sd="([^"]*)"/.exec(tag)?.[1] ?? '' }
			: {
					kind: 'ellipse',
					cx: number(tag, 'cx'),
					cy: number(tag, 'cy'),
					rx: number(tag, name === 'circle' ? 'r' : 'rx'),
					ry: number(tag, name === 'circle' ? 'r' : 'ry')
				}
	);
}

const TAKEOFF = shapesOf(takeoffFile);
const LANDING = shapesOf(landingFile);

/**
 * The pack's icon in the pin's head: the take-off's for a place to take
 * off from, a winch's among them, the landing's for a landing; none for a
 * training slope or the rest, which have a ring.
 */
export function iconOf(kind: SiteKind): Shape[] {
	if (ringed(kind)) return [];
	return kind === 'landing' ? LANDING : TAKEOFF;
}

/**
 * The pictures of every kind of site, for the map, at the screen's pixel
 * ratio: each as it is, and muted for one that stands apart.
 */
export function sitePictures(pixelRatio = 2): Picture[] {
	return [false, true].flatMap((apart) =>
		SITE_KINDS.map((kind) => ({
			name: pictureName(kind, apart),
			image: draw(kind, apart, pixelRatio),
			pixelRatio
		}))
	);
}

function draw(kind: SiteKind, apart: boolean, pixelRatio: number): ImageData {
	const canvas = document.createElement('canvas');
	canvas.width = PIN_WIDTH * pixelRatio;
	canvas.height = PIN_HEIGHT * pixelRatio;
	const context = canvas.getContext('2d', { willReadFrequently: true })!;
	context.scale(pixelRatio, pixelRatio);
	const { ground, sign } = colours(kind, apart);

	const pin = new Path2D(PIN);
	context.fillStyle = ground;
	context.fill(pin);
	context.lineWidth = EDGE_WIDTH;
	context.strokeStyle = sign;
	context.stroke(pin);

	if (ringed(kind)) {
		const mark = new Path2D();
		mark.arc(MARK.x, MARK.y, MARK.radius, 0, 2 * Math.PI);
		context.lineWidth = RING_WIDTH;
		context.stroke(mark);
	} else {
		// The pack's icon, as its file draws it: a 2-point stroke, round ends.
		context.translate(ICON_AT.x, ICON_AT.y);
		context.scale(ICON_SCALE, ICON_SCALE);
		context.lineWidth = 2;
		context.lineCap = 'round';
		context.lineJoin = 'round';
		for (const shape of iconOf(kind)) {
			const path = new Path2D(shape.kind === 'path' ? shape.d : undefined);
			if (shape.kind === 'ellipse') {
				path.ellipse(shape.cx, shape.cy, shape.rx, shape.ry, 0, 0, 2 * Math.PI);
			}
			context.stroke(path);
		}
	}
	return context.getImageData(0, 0, canvas.width, canvas.height);
}
