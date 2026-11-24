/**
 * Take-offs on the map as small fans over the wind directions they work
 * in, as Burnair draws them: a wedge for each span the source gives,
 * north up, under the take-off's pin. While the map shows the forecast's
 * wind at the ground for an hour, each fan is coloured by whether that
 * wind there comes from one of its directions: the pack's good colour
 * where it does, its caution colour where it does not; otherwise, and for
 * a calm hour or a place the forecast has nothing for, one neutral ink.
 *
 * Which directions a site works in is the source's word, read by the core
 * (`SiteView.wind`); the forecast's wind at a take-off is the core's for
 * the square it lies in (`ThermalMapHours.windAt`, through the worker's
 * `thermalWindsAt`). This draws them and compares the two.
 */

import type { LayerSpecification } from 'maplibre-gl';
import type { SiteView, ThermalMapWind, WindSpan } from '#lib/core/index.ts';
import { CAUTION_ON_MAP, GOOD_ON_MAP, INK_ON_MAP, PAPER_ON_MAP } from '#lib/design/palette.ts';
import { wedges } from './rose';
import { SITES_SOURCE } from './mark';
import { MARK, PIN_HEIGHT } from './symbols';
import { SITES_MIN_ZOOM } from './watch';

/**
 * The map's source of the fans and its layer, under the pins. Its name
 * begins as the pins' layer does, so that the map's "Sites" chip switches
 * it with them.
 */
export const FANS_SOURCE = `${SITES_SOURCE}-fans`;

/**
 * The fan's radius on the screen, in pixels: around the pin's head, which
 * it is centred on, reaching out past it on every side.
 */
export const FAN_RADIUS = 22;
/** How see-through a fan's wedges are, so that the map shows under them. */
const FAN_FILL = 0.55;
const FAN_EDGE = 1.5;

/** What a fan says of the wind of the hour shown. */
export type Fit = 'fits' | 'misfits' | 'neutral';

/** The colour a fan is drawn in. */
export function fitColour(fit: Fit): string {
	if (fit === 'fits') return GOOD_ON_MAP;
	if (fit === 'misfits') return CAUTION_ON_MAP;
	return INK_ON_MAP;
}

/** The width a single direction is taken to cover: the sixteenth of the compass it names. */
const SINGLE_DEG = 22.5;

/** Whether a wind from `fromDeg` comes from one of the spans. */
export function windFits(spans: readonly WindSpan[], fromDeg: number): boolean {
	const from = ((fromDeg % 360) + 360) % 360;
	return spans.some((span) => {
		const given = span.toDeg - span.fromDeg;
		if (given >= 360) return true;
		const width = ((given % 360) + 360) % 360;
		const [start, end] =
			width === 0
				? [span.fromDeg - SINGLE_DEG / 2, span.fromDeg + SINGLE_DEG / 2]
				: [span.fromDeg, span.fromDeg + width];
		const past = (((from - start) % 360) + 360) % 360;
		return past <= end - start;
	});
}

/** The forecast's wind at a place: where it comes from, calm, or nothing known. */
export type GroundWind = { fromDeg: number } | 'calm' | undefined;

/**
 * The core's wind at a take-off as a fan reads it: where it comes from,
 * calm (the core's `calm`, or no direction), or nothing known.
 */
export function groundWindOf(wind: ThermalMapWind | undefined): GroundWind {
	if (!wind) return undefined;
	if (wind.calm || wind.fromDeg === undefined || wind.fromDeg === null) return 'calm';
	return { fromDeg: wind.fromDeg };
}

/** What a fan says of a wind: nothing to say without one, or in a calm. */
export function fitOf(spans: readonly WindSpan[], wind: GroundWind): Fit {
	if (wind === undefined || wind === 'calm') return 'neutral';
	return windFits(spans, wind.fromDeg) ? 'fits' : 'misfits';
}

/** The spans in a name of their own: "0-45,180-180". */
function spansKey(spans: readonly WindSpan[]): string {
	return spans.map((span) => `${span.fromDeg}-${span.toDeg}`).join(',');
}

const PREFIX = 'site-fan:';

/** The name of a fan's picture: what it says and its spans, which the picture is drawn from. */
export function fanName(spans: readonly WindSpan[], fit: Fit): string {
	return `${PREFIX}${fit}:${spansKey(spans)}`;
}

/** What a fan's picture shows, read back from its name; nothing for another picture. */
export function fanOf(name: string): { fit: Fit; spans: WindSpan[] } | undefined {
	if (!name.startsWith(PREFIX)) return undefined;
	const [fit, key = ''] = name.slice(PREFIX.length).split(':');
	if (fit !== 'fits' && fit !== 'misfits' && fit !== 'neutral') return undefined;
	const spans = key
		.split(',')
		.filter(Boolean)
		.map((pair) => {
			const [fromDeg, toDeg] = pair.split('-').map(Number);
			return { fromDeg, toDeg };
		})
		.filter((span) => Number.isFinite(span.fromDeg) && Number.isFinite(span.toDeg));
	return spans.length > 0 ? { fit, spans } : undefined;
}

/**
 * The fans of the take-offs whose source gives their directions, as the
 * layer reads them. A landing has its own pin and no fan.
 */
export function fanFeatures(
	sites: readonly SiteView[],
	winds?: ReadonlyMap<string, GroundWind>
): GeoJSON.FeatureCollection {
	return {
		type: 'FeatureCollection',
		features: sites
			.filter((site) => site.uses.includes('takeoff') && site.wind.length > 0)
			.map((site) => ({
				type: 'Feature' as const,
				geometry: { type: 'Point' as const, coordinates: [site.position.lon, site.position.lat] },
				properties: {
					id: site.id,
					picture: fanName(site.wind, winds ? fitOf(site.wind, winds.get(site.id)) : 'neutral')
				}
			}))
	};
}

/** The layer that draws the fans: north up on the ground, turning and leaning with the map. */
export function fansLayer(): LayerSpecification {
	return {
		id: FANS_SOURCE,
		type: 'symbol',
		source: FANS_SOURCE,
		minzoom: SITES_MIN_ZOOM,
		layout: {
			'icon-image': ['get', 'picture'],
			'icon-rotation-alignment': 'map',
			'icon-pitch-alignment': 'map',
			'icon-allow-overlap': true,
			'icon-ignore-placement': true
		}
	};
}

/**
 * A fan's picture, at the screen's pixel ratio: its wedges in its colour,
 * edged in the map's paper, round the pin's head.
 */
export function fanPicture(
	spans: readonly WindSpan[],
	fit: Fit,
	pixelRatio = 2
): { image: ImageData; pixelRatio: number } {
	const side = (FAN_RADIUS + FAN_EDGE) * 2;
	// The fan stands round the pin's head, which is this far over the site:
	// round the pin's tip it would hide under the pin where the wind is
	// north. The picture is centred on the site, so it reaches as far below.
	const lift = PIN_HEIGHT - MARK.y;
	const canvas = document.createElement('canvas');
	canvas.width = side * pixelRatio;
	canvas.height = (side + 2 * lift) * pixelRatio;
	const context = canvas.getContext('2d', { willReadFrequently: true })!;
	context.scale(pixelRatio, pixelRatio);
	const colour = fitColour(fit);
	context.lineJoin = 'round';
	for (const d of wedges(spans, side / 2, FAN_RADIUS)) {
		const path = new Path2D(d);
		context.globalAlpha = FAN_FILL;
		context.fillStyle = colour;
		context.fill(path);
		context.globalAlpha = 1;
		context.lineWidth = FAN_EDGE;
		context.strokeStyle = PAPER_ON_MAP;
		context.stroke(path);
	}
	return { image: context.getImageData(0, 0, canvas.width, canvas.height), pixelRatio };
}
