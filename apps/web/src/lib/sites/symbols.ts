/**
 * The pins the map draws flying sites with, as the design pack draws them:
 * the pack's indigo and cream, with the tip on the site, so that a site is
 * never taken for wind, airspace or a track, which have the colours. The
 * iPhone and Android draw the same.
 *
 * `SiteGlyph.svelte` draws the same pin in a list, so the map and the list
 * show one sign.
 */

import type { SiteKind } from '#lib/core/index.ts';
import { INK_ON_MAP, PAPER_ON_MAP } from '#lib/design/palette.ts';
import type { Picture } from '#lib/wind/symbols.ts';

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

export function pictureName(kind: SiteKind): string {
	return `site-${kind}`;
}

/**
 * How a kind of site is drawn, in the pack's indigo and cream: a place to
 * take off from is a dark pin with a cream dot, a landing the same turned
 * round, cream with a dark dot; a training slope or a site of another kind
 * has a ring in place of the dot.
 */
export function colours(kind: SiteKind): { ground: string; sign: string } {
	return kind === 'landing'
		? { ground: PAPER_ON_MAP, sign: INK_ON_MAP }
		: { ground: INK_ON_MAP, sign: PAPER_ON_MAP };
}

/** Whether the pin's head holds a ring rather than a dot. */
export function ringed(kind: SiteKind): boolean {
	return kind === 'training' || kind === 'other';
}

/** The pictures of every kind of site, for the map, at the screen's pixel ratio. */
export function sitePictures(pixelRatio = 2): Picture[] {
	return SITE_KINDS.map((kind) => ({
		name: pictureName(kind),
		image: draw(kind, pixelRatio),
		pixelRatio
	}));
}

function draw(kind: SiteKind, pixelRatio: number): ImageData {
	const canvas = document.createElement('canvas');
	canvas.width = PIN_WIDTH * pixelRatio;
	canvas.height = PIN_HEIGHT * pixelRatio;
	const context = canvas.getContext('2d', { willReadFrequently: true })!;
	context.scale(pixelRatio, pixelRatio);
	const { ground, sign } = colours(kind);

	const pin = new Path2D(PIN);
	context.fillStyle = ground;
	context.fill(pin);
	context.lineWidth = EDGE_WIDTH;
	context.strokeStyle = sign;
	context.stroke(pin);

	const mark = new Path2D();
	mark.arc(MARK.x, MARK.y, MARK.radius, 0, 2 * Math.PI);
	if (ringed(kind)) {
		context.lineWidth = RING_WIDTH;
		context.stroke(mark);
	} else {
		context.fillStyle = sign;
		context.fill(mark);
	}
	return context.getImageData(0, 0, canvas.width, canvas.height);
}
