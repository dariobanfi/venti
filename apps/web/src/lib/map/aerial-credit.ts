/**
 * Whose the photographs of the aerial view are, in a short line, for a map
 * whose own credits are folded behind their button (a phone's width):
 * "Photos: basemap.at · Bayerische Vermessungsverwaltung · EOX". The
 * providers' full wording is behind that button.
 *
 * Only the makers of photographs that can show on the part of the map on
 * screen are named, as `docs/design.md` says: each aerial layer of the
 * shared style names its maker in its metadata (`venti:credit`), its
 * source says where it has photographs (`bounds`), and the layer from
 * which zoom it shows (`minzoom`). This file imports nothing of the map
 * engine, so that the pages can use it before the map is loaded.
 */

/** A part of the map, in degrees. */
export interface Area {
	south: number;
	west: number;
	north: number;
	east: number;
}

/** One layer of photographs: who made them, where they are, and from which zoom they show. */
export interface AerialMaker {
	credit: string;
	/** Where the source has photographs; none for the whole world. */
	area?: Area;
	minzoom: number;
}

/** As much of a map style as the line needs. */
interface StyleLike {
	sources: Record<string, unknown>;
	layers: readonly unknown[];
}

/** The aerial view's layers of photographs in a style, in the style's order. */
export function aerialMakers(style: StyleLike): AerialMaker[] {
	const makers: AerialMaker[] = [];
	for (const each of style.layers) {
		const layer = each as {
			type?: string;
			source?: string;
			minzoom?: number;
			metadata?: Record<string, unknown>;
		};
		const credit = layer.metadata?.['venti:credit'];
		if (layer.type !== 'raster' || layer.metadata?.['venti:view'] !== 'aerial') continue;
		if (typeof credit !== 'string' || layer.source === undefined) continue;
		const source = style.sources[layer.source] as
			{ bounds?: [number, number, number, number] } | undefined;
		if (!source) continue;
		const bounds = source.bounds;
		makers.push({
			credit,
			area: bounds && { west: bounds[0], south: bounds[1], east: bounds[2], north: bounds[3] },
			minzoom: layer.minzoom ?? 0
		});
	}
	return makers;
}

function meet(one: Area, other: Area): boolean {
	return (
		one.south <= other.north &&
		other.south <= one.north &&
		one.west <= other.east &&
		other.west <= one.east
	);
}

/**
 * The line for the part of the map on screen at this zoom: the makers whose
 * photographs can show there, those of a region first in the style's order,
 * those of the whole world last, each once. Empty where none can show.
 */
export function photosCredit(makers: readonly AerialMaker[], visible: Area, zoom: number): string {
	const showing = makers.filter(
		(maker) => zoom >= maker.minzoom && (!maker.area || meet(maker.area, visible))
	);
	const names = [
		...showing.filter((maker) => maker.area),
		...showing.filter((maker) => !maker.area)
	].map((maker) => maker.credit);
	const once = [...new Set(names)];
	return once.length === 0 ? '' : `Photos: ${once.join(' · ')}`;
}
