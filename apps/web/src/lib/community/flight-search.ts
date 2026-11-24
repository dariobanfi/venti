/**
 * A search among public flights as the address carries it, so that a link
 * shares it: the take-off within a radius of a place or inside an area
 * drawn on the map, the glider's class, the kind of route, the least
 * distance and the take-off's country. The names are the server's own
 * (`docs/api.md`): `?near=47.6642,11.7967&radius_km=10&class=en_c`.
 *
 * What the search means, its bounds and its words are the core's
 * (`FlightSearch`, `flightSearchWords`, `flightSearchBounds`, asked in the
 * worker). This file only writes the choice into an address and reads it
 * back, and draws a circle for the map.
 */

import type { Coordinate, FlightSearch, GliderClassKind, RouteKind } from '#lib/core/index.ts';

/** One choice of a search, which a chip shows and its × lets go of. */
export type SearchPart = 'near' | 'area' | 'gliderClass' | 'kind' | 'minKm' | 'country';

/** The parts in the order a filter bar shows them, as the core's words come. */
export const SEARCH_PARTS: readonly SearchPart[] = [
	'near',
	'area',
	'gliderClass',
	'kind',
	'minKm',
	'country'
];

/** The search of everything. */
export const NO_SEARCH: FlightSearch = {};

/** The address's names of the classes, the server's. */
const CLASSES: Record<GliderClassKind, string> = {
	a: 'en_a',
	b: 'en_b',
	c: 'en_c',
	d: 'en_d',
	ccc: 'ccc'
};

/** The address's names of the kinds of route, the server's. */
const KINDS: Record<RouteKind, string> = {
	freeFlight: 'free_flight',
	freeTriangle: 'free_triangle',
	faiTriangle: 'fai_triangle'
};

/** The kinds of route, in the order a menu lists them. */
export const ROUTE_KINDS: readonly RouteKind[] = ['freeFlight', 'freeTriangle', 'faiTriangle'];

function keyOf<T extends string>(names: Record<T, string>, value: string | null): T | undefined {
	return (Object.keys(names) as T[]).find((key) => names[key] === value);
}

/** Degrees as the address writes them: to about a metre, which is more than a take-off needs. */
function degrees(value: number): string {
	return String(Math.round(value * 100_000) / 100_000);
}

function place(lat: number, lon: number): Coordinate | undefined {
	return Number.isFinite(lat) && Number.isFinite(lon) && Math.abs(lat) <= 90 && Math.abs(lon) <= 180
		? { lat, lon }
		: undefined;
}

/** Numbers written one after the other with commas, or nothing where one is not a number. */
function numbers(text: string): number[] | undefined {
	if (!text.trim()) return undefined;
	const parts = text.split(',').map((part) => (part.trim() ? Number(part) : Number.NaN));
	return parts.every(Number.isFinite) ? parts : undefined;
}

/**
 * The search an address asks for. What is not written as the server writes
 * it is left out; whether the rest is within the bounds is the core's to
 * say when the list is asked for, in words.
 */
export function searchIn(query: Pick<URLSearchParams, 'get'>): FlightSearch {
	const search: FlightSearch = {};
	const near = numbers(query.get('near') ?? '');
	const radius = Number(query.get('radius_km') ?? '');
	if (near?.length === 2 && Number.isFinite(radius) && radius > 0) {
		const at = place(near[0], near[1]);
		if (at) {
			search.near = at;
			search.radiusKm = radius;
		}
	}
	const corners = numbers(query.get('area') ?? '');
	if (corners && corners.length >= 6 && corners.length % 2 === 0) {
		const area: Coordinate[] = [];
		for (let index = 0; index < corners.length; index += 2) {
			const at = place(corners[index], corners[index + 1]);
			if (!at) break;
			area.push(at);
		}
		if (area.length * 2 === corners.length) search.area = area;
	}
	const gliderClass = keyOf(CLASSES, query.get('class'));
	if (gliderClass) search.gliderClass = gliderClass;
	const kind = keyOf(KINDS, query.get('kind'));
	if (kind) search.kind = kind;
	const min = query.get('min_km');
	if (min !== null && min.trim() !== '' && Number.isFinite(Number(min)) && Number(min) > 0) {
		search.minKm = Number(min);
	}
	const country = (query.get('country') ?? '').trim().toUpperCase();
	if (/^[A-Z]{2}$/.test(country)) search.country = country;
	return search;
}

/** The search written into an address's query, without the `?`: empty for a search of everything. */
export function searchParams(search: FlightSearch | undefined): URLSearchParams {
	const query = new URLSearchParams();
	if (!search) return query;
	if (search.near && search.radiusKm !== undefined) {
		query.set('near', `${degrees(search.near.lat)},${degrees(search.near.lon)}`);
		query.set('radius_km', String(search.radiusKm));
	}
	if (search.area && search.area.length > 0) {
		query.set(
			'area',
			search.area.map((corner) => `${degrees(corner.lat)},${degrees(corner.lon)}`).join(',')
		);
	}
	if (search.gliderClass) query.set('class', CLASSES[search.gliderClass]);
	if (search.kind) query.set('kind', KINDS[search.kind]);
	if (search.minKm !== undefined) query.set('min_km', String(search.minKm));
	if (search.country) query.set('country', search.country);
	return query;
}

/** Whether a search chooses anything at all. */
export function isSearching(search: FlightSearch | undefined): boolean {
	return searchParams(search).size > 0;
}

/** The same search without one of its choices: what a chip's × asks for. */
export function without(search: FlightSearch, part: SearchPart): FlightSearch {
	const rest = { ...search };
	switch (part) {
		case 'near':
			delete rest.near;
			delete rest.radiusKm;
			break;
		case 'area':
			delete rest.area;
			break;
		default:
			delete rest[part];
	}
	return rest;
}

/** The search of one choice alone, for that choice's chip. */
export function only(search: FlightSearch, part: SearchPart): FlightSearch {
	switch (part) {
		case 'near':
			return search.near && search.radiusKm !== undefined
				? { near: search.near, radiusKm: search.radiusKm }
				: {};
		case 'area':
			return search.area && search.area.length > 0 ? { area: search.area } : {};
		default:
			return search[part] === undefined ? {} : { [part]: search[part] };
	}
}

/** Whether two searches choose the same, as their addresses say. */
export function sameSearch(
	one: FlightSearch | undefined,
	other: FlightSearch | undefined
): boolean {
	return searchParams(one).toString() === searchParams(other).toString();
}

/** The mean radius of the Earth the map is drawn on, metres. */
const EARTH_M = 6_371_008.8;

/** How far apart two places are on the sphere, kilometres. */
export function kilometresBetween(one: Coordinate, other: Coordinate): number {
	const rad = Math.PI / 180;
	const dLat = (other.lat - one.lat) * rad;
	const dLon = (other.lon - one.lon) * rad;
	const a =
		Math.sin(dLat / 2) ** 2 +
		Math.cos(one.lat * rad) * Math.cos(other.lat * rad) * Math.sin(dLon / 2) ** 2;
	return (2 * EARTH_M * Math.asin(Math.min(1, Math.sqrt(a)))) / 1000;
}

/** The place so many kilometres from another in a direction, degrees clockwise from north. */
export function placeFrom(start: Coordinate, km: number, bearingDeg: number): Coordinate {
	const rad = Math.PI / 180;
	const angle = (km * 1000) / EARTH_M;
	const bearing = bearingDeg * rad;
	const lat1 = start.lat * rad;
	const lon1 = start.lon * rad;
	const lat2 = Math.asin(
		Math.sin(lat1) * Math.cos(angle) + Math.cos(lat1) * Math.sin(angle) * Math.cos(bearing)
	);
	const lon2 =
		lon1 +
		Math.atan2(
			Math.sin(bearing) * Math.sin(angle) * Math.cos(lat1),
			Math.cos(angle) - Math.sin(lat1) * Math.sin(lat2)
		);
	return { lat: lat2 / rad, lon: ((lon2 / rad + 540) % 360) - 180 };
}

/** A circle on the map as a ring of corners, longitude then latitude, closed. */
export function circleRing(centre: Coordinate, km: number, corners = 72): [number, number][] {
	const ring: [number, number][] = [];
	for (let index = 0; index <= corners; index += 1) {
		const at = placeFrom(centre, km, (index * 360) / corners);
		ring.push([at.lon, at.lat]);
	}
	return ring;
}

/** An address with a search added to whatever its query already asks. */
export function withSearch(href: string, search: FlightSearch | undefined): string {
	const added = searchParams(search).toString();
	if (!added) return href;
	return `${href}${href.includes('?') ? '&' : '?'}${added}`;
}
