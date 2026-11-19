/**
 * The signs the map draws flying sites with: a dark disc with a white sign
 * for a take-off, a white disc with a dark sign for a landing, so that the
 * two are told apart at a glance and neither is taken for wind, airspace or
 * a track, which have the colours. The iPhone app draws the same in
 * `SiteScreens.swift`.
 *
 * `SIGNS` is also what `SiteGlyph.svelte` draws in a list, so the map and
 * the list show one sign.
 */

import type { SiteKind } from '#lib/core/index.ts';
import { INK_ON_MAP } from '#lib/design/palette.ts';
import type { Picture } from '#lib/wind/symbols.ts';

/** The side of a picture, in CSS pixels. */
export const SIDE = 26;

/** The radius of the disc, and the width of its edge. */
export const DISC_RADIUS = 11;
export const EDGE_WIDTH = 1.5;

/** The side of the box the signs are drawn in, and how wide a sign is on the disc. */
export const SIGN_BOX = 24;
export const SIGN_SIDE = 14;
/** The width of a sign's lines, in the box's own units. */
export const SIGN_LINE = 3.6;

/** Every kind, for making the map's pictures. */
export const SITE_KINDS: readonly SiteKind[] = [
	'takeoff',
	'landing',
	'towing',
	'training',
	'other'
];

/** What stands on the disc of each kind: lines in a box of 24, as SVG paths. */
export const SIGNS: Readonly<Record<SiteKind, string>> = {
	// An arrow up and away, and one down to the ground.
	takeoff: 'M7 17 17 7M9 7h8v8',
	landing: 'M7 7l10 10M17 9v8H9',
	// An arrow up to a line: pulled into the air on a rope.
	towing: 'M6.5 5h11M12 19.5V10M7.5 14 12 9.5l4.5 4.5',
	// A graduate's cap.
	training:
		'M3 10l9-4.5 9 4.5-9 4.5-9-4.5ZM7.5 13.5v3.2c1.2 1.1 2.7 1.8 4.5 1.8s3.3-.7 4.5-1.8v-3.2',
	// A pin: a place, and no more is known.
	other: 'M12 20.5c-3.7-3.6-5.5-6.8-5.5-9.5a5.5 5.5 0 0 1 11 0c0 2.7-1.8 5.9-5.5 9.5ZM12 10.9h.01'
};

/** The name the map knows a kind's picture by. */
export function pictureName(kind: SiteKind): string {
	return `site-${kind}`;
}

/** The disc's colour, and the colour of its sign and its edge. A landing is the light one. */
export function colours(kind: SiteKind): { ground: string; sign: string } {
	return kind === 'landing'
		? { ground: '#ffffff', sign: INK_ON_MAP }
		: { ground: INK_ON_MAP, sign: '#ffffff' };
}

/** Every picture the sites layer uses, drawn sharp for dense screens. */
export function sitePictures(pixelRatio = 2): Picture[] {
	return SITE_KINDS.map((kind) => ({
		name: pictureName(kind),
		image: draw(kind, pixelRatio),
		pixelRatio
	}));
}

function draw(kind: SiteKind, pixelRatio: number): ImageData {
	const canvas = document.createElement('canvas');
	canvas.width = canvas.height = SIDE * pixelRatio;
	const context = canvas.getContext('2d', { willReadFrequently: true })!;
	context.scale(pixelRatio, pixelRatio);
	const { ground, sign } = colours(kind);

	const disc = new Path2D();
	disc.arc(SIDE / 2, SIDE / 2, DISC_RADIUS, 0, 2 * Math.PI);
	// A soft shadow lifts the disc off any ground.
	context.shadowColor = 'rgb(0 0 0 / 0.35)';
	context.shadowBlur = 1.5 * pixelRatio;
	context.shadowOffsetY = 0.5 * pixelRatio;
	context.fillStyle = ground;
	context.fill(disc);
	context.shadowColor = 'transparent';
	context.lineWidth = EDGE_WIDTH;
	context.strokeStyle = sign;
	context.stroke(disc);

	const scale = SIGN_SIDE / SIGN_BOX;
	context.translate(SIDE / 2, SIDE / 2);
	context.scale(scale, scale);
	context.translate(-SIGN_BOX / 2, -SIGN_BOX / 2);
	context.lineWidth = SIGN_LINE;
	context.lineCap = 'round';
	context.lineJoin = 'round';
	context.stroke(new Path2D(SIGNS[kind]));

	return context.getImageData(0, 0, canvas.width, canvas.height);
}
