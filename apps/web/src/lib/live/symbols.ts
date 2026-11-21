/**
 * The pictures the map draws other pilots with: a paraglider seen from
 * above, its wing as a crescent and the pilot as a dot behind it, green
 * while it flies and grey when nothing was heard of it for a while; and a
 * grey disc for a glider on the ground. The iPhone app draws the same
 * shapes in `LiveModel.swift`.
 *
 * `WING` and `PILOT` are also what `GliderGlyph.svelte` draws in a list, so
 * the map and the list show one glider.
 */

import type { LiveGlider } from '#lib/core/index.ts';
import {
	FLYING_ON_MAP,
	MUTED_INK_ON_MAP,
	PAPER_ON_MAP,
	QUIET_ON_MAP
} from '#lib/design/palette.ts';
import type { Picture } from '#lib/wind/symbols.ts';

/** The side of a picture, in CSS pixels. */
export const SIDE = 30;

/**
 * The wing, flying north, as an SVG path in a box of {@link SIDE}: the
 * leading edge from tip over the middle to tip, then the trailing edge
 * back, flatter. The map turns the picture to the glider's course.
 */
export const WING = 'M3.22 15.51A13 13 0 0 1 26.78 15.51A24.56 24.56 0 0 0 3.22 15.51Z';

/** The pilot behind the wing: the middle of the dot and its radius. */
export const PILOT = { x: 15, y: 20, radius: 2.6 } as const;

/** A glider on the ground: the radius of its disc, and the width of the disc's white edge. */
export const DISC_RADIUS = 5.5;
export const DISC_EDGE = 2;

/** How a glider is drawn: flying, not heard of for a while, or on the ground. */
export type GliderLook = 'flying' | 'quiet' | 'landed';

/**
 * Which picture a glider is drawn with. Whether it has landed and whether
 * it has gone quiet are the core's to say; a quiet glider is never drawn
 * as landed.
 */
export function lookOf(glider: Pick<LiveGlider, 'landed' | 'quiet'>): GliderLook {
	if (glider.landed) return 'landed';
	return glider.quiet ? 'quiet' : 'flying';
}

/** The name the map knows a look's picture by. */
export function pictureName(look: GliderLook): string {
	return `live-${look}`;
}

/**
 * Green for a glider that is flying: none of the colours of wind, airspace
 * or the pilot's own tracks. Grey for one that is on the ground, or that
 * nothing was heard of for a while.
 */
export function colourOf(look: GliderLook): string {
	return look === 'flying' ? FLYING_ON_MAP : QUIET_ON_MAP;
}

/**
 * The picture of an aircraft another network hears (the Open Glider
 * Network): the same wing, in the pack's muted ink, so that it is told
 * apart from Venti's own pilots at a glance.
 */
export const OTHER_PICTURE = 'live-other';

/** Every picture the gliders' layers use, drawn sharp for dense screens. */
export function gliderPictures(pixelRatio = 2): Picture[] {
	const pilots = (['flying', 'quiet', 'landed'] as const).map((look) => ({
		name: pictureName(look),
		image: look === 'landed' ? disc(pixelRatio) : wing(colourOf(look), pixelRatio),
		pixelRatio
	}));
	return [
		...pilots,
		{ name: OTHER_PICTURE, image: wing(MUTED_INK_ON_MAP, pixelRatio), pixelRatio }
	];
}

function canvas(pixelRatio: number): CanvasRenderingContext2D {
	const canvas = document.createElement('canvas');
	canvas.width = canvas.height = SIDE * pixelRatio;
	const context = canvas.getContext('2d', { willReadFrequently: true })!;
	context.scale(pixelRatio, pixelRatio);
	return context;
}

/** A soft shadow that lifts a shape off any ground. */
function shadow(context: CanvasRenderingContext2D, pixelRatio: number, on: boolean): void {
	context.shadowColor = on ? 'rgb(0 0 0 / 0.35)' : 'transparent';
	context.shadowBlur = on ? 1.5 * pixelRatio : 0;
	context.shadowOffsetY = on ? 0.5 * pixelRatio : 0;
}

function wing(colour: string, pixelRatio: number): ImageData {
	const context = canvas(pixelRatio);
	const pilot = new Path2D();
	pilot.arc(PILOT.x, PILOT.y, PILOT.radius, 0, 2 * Math.PI);
	const shapes = [new Path2D(WING), pilot];

	// A white edge keeps the glider readable over forest, rock and snow.
	shadow(context, pixelRatio, true);
	context.lineJoin = 'round';
	context.lineWidth = 3;
	context.strokeStyle = PAPER_ON_MAP;
	for (const shape of shapes) context.stroke(shape);
	shadow(context, pixelRatio, false);
	context.fillStyle = colour;
	for (const shape of shapes) context.fill(shape);

	return context.getImageData(0, 0, SIDE * pixelRatio, SIDE * pixelRatio);
}

function disc(pixelRatio: number): ImageData {
	const context = canvas(pixelRatio);
	const disc = new Path2D();
	disc.arc(SIDE / 2, SIDE / 2, DISC_RADIUS, 0, 2 * Math.PI);

	shadow(context, pixelRatio, true);
	context.fillStyle = colourOf('landed');
	context.fill(disc);
	shadow(context, pixelRatio, false);
	context.lineWidth = DISC_EDGE;
	context.strokeStyle = PAPER_ON_MAP;
	context.stroke(disc);

	return context.getImageData(0, 0, SIDE * pixelRatio, SIDE * pixelRatio);
}
