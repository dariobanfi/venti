/**
 * A light casing under the app's own dark lines while photographs show.
 *
 * Over the drawn map a dark line stands out by itself; over a photograph of
 * forest and rock it does not. So every plain dark line the app draws on a
 * map has, under it, a line of the map's label halo (`map.light.label-halo`
 * of the design tokens) that is about 1.5 px wider on each side, hidden
 * while the map is drawn and shown with the aerial view (`setAerial`). The
 * flight's own track keeps its dark casing alone: its climb colours carry it.
 */

import type { LineLayerSpecification, Map } from 'maplibre-gl';
import { PAPER_ON_MAP } from '#lib/design/palette.ts';

/** How much wider the casing is than the line it lies under, in pixels: 1.5 on each side. */
export const CASING_WIDENS = 3;

/** The end of a casing's name, after the name of the line it is under. */
export const CASING = '-aerial-casing';

/**
 * The lines that get a casing: the logbook's tracks on the map tab, a
 * followed pilot's track, the route planner's drawn and scored lines, and
 * on a flight's page the scored route and the other flight's line. Each by
 * the name of its outermost dark layer.
 */
export const CASED: readonly string[] = [
	'logbook-tracks-edge',
	'live-track-edge',
	'route-drawn-line',
	'route-scored-line',
	'flight-route-line',
	'other-track-edge'
];

/** The casing of a line: solid, in the label halo's colour, hidden until the aerial view. */
export function casingOf(line: LineLayerSpecification): LineLayerSpecification {
	const width = line.paint?.['line-width'];
	return {
		id: `${line.id}${CASING}`,
		type: 'line',
		source: line.source,
		...(line.filter ? { filter: line.filter } : {}),
		layout: { ...line.layout, visibility: 'none' },
		paint: {
			'line-color': PAPER_ON_MAP,
			'line-width': typeof width === 'number' ? width + CASING_WIDENS : CASING_WIDENS
		}
	};
}

/** Adds a line to a map, with its casing under it where it is one of `CASED`. */
export function addLine(map: Pick<Map, 'addLayer'>, line: LineLayerSpecification): void {
	if (CASED.includes(line.id)) map.addLayer(casingOf(line));
	map.addLayer(line);
}

/** The names of the casings on a map: what the aerial view shows and the drawn map hides. */
export function casingsOn(map: Pick<Map, 'getStyle'>): string[] {
	return (map.getStyle().layers ?? []).map((layer) => layer.id).filter((id) => id.endsWith(CASING));
}
