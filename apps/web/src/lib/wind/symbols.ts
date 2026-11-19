/**
 * The pictures the map draws wind stations with: a dart for each band of
 * wind, a pale dot for calm and a grey dot for a station with nothing
 * current to say. The iPhone app draws the same shapes in `WindBand.swift`.
 *
 * `DART` is also what `WindGlyph.svelte` draws in a list, so the map and
 * the list show one arrow.
 */

import type { WindBand } from '#lib/core/index.ts';
import { CALM_ON_MAP, OLD_ON_MAP, WIND_BANDS } from '#lib/design/palette.ts';
import { pictureName } from './mark';

/** The side of a picture, in CSS pixels. */
export const SIDE = 30;

/** A dart pointing up, as corners in shares of the picture's side. */
export const DART: readonly (readonly [number, number])[] = [
	[0.5, 0.13],
	[0.78, 0.87],
	[0.5, 0.69],
	[0.22, 0.87]
];

/** The radius of a dot, in CSS pixels. */
export const DOT_RADIUS = 4.5;

/** A picture with the name the map knows it by. */
export interface Picture {
	name: string;
	image: ImageData;
	/** How many of the image's pixels make one CSS pixel. */
	pixelRatio: number;
}

/** Every picture the wind layer uses, drawn sharp for dense screens. */
export function windPictures(pixelRatio = 2): Picture[] {
	const dart = (color: string) =>
		draw(pixelRatio, color, (path) => {
			DART.forEach(([x, y], corner) =>
				corner === 0 ? path.moveTo(x * SIDE, y * SIDE) : path.lineTo(x * SIDE, y * SIDE)
			);
			path.closePath();
		});
	const dot = (color: string) =>
		draw(pixelRatio, color, (path) => path.arc(SIDE / 2, SIDE / 2, DOT_RADIUS, 0, 2 * Math.PI));

	return [
		...(Object.keys(WIND_BANDS) as WindBand[]).map((band) => ({
			name: pictureName({ kind: 'arrow', band }),
			image: dart(WIND_BANDS[band].color),
			pixelRatio
		})),
		{ name: pictureName({ kind: 'calm' }), image: dot(CALM_ON_MAP), pixelRatio },
		{ name: pictureName({ kind: 'old' }), image: dot(OLD_ON_MAP), pixelRatio }
	];
}

/**
 * A shape filled with a colour, with a white edge and a soft shadow so that
 * it stands out on any ground.
 */
function draw(pixelRatio: number, color: string, shape: (path: Path2D) => void): ImageData {
	const canvas = document.createElement('canvas');
	canvas.width = canvas.height = SIDE * pixelRatio;
	const context = canvas.getContext('2d', { willReadFrequently: true })!;
	context.scale(pixelRatio, pixelRatio);

	const path = new Path2D();
	shape(path);

	context.lineJoin = 'round';
	context.lineWidth = 3.5;
	context.strokeStyle = '#ffffff';
	context.shadowColor = 'rgb(0 0 0 / 0.35)';
	context.shadowBlur = 1.5 * pixelRatio;
	context.shadowOffsetY = 0.5 * pixelRatio;
	context.stroke(path);

	context.shadowColor = 'transparent';
	context.fillStyle = color;
	context.fill(path);

	return context.getImageData(0, 0, canvas.width, canvas.height);
}
