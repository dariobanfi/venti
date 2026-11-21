/**
 * The geometry of a flight in the air, for the flight map's 3D view: where
 * each position lies in a local frame, the ribbon of the track, the curtain
 * down to the ground, the thermals' columns and where their words go.
 *
 * Pure functions, so that they are tested on their own; `AirLayer.ts` only
 * hands what they make to WebGL. What a thermal or a glide is, and how much
 * it gained or lost, is the core's analysis; nothing here decides it.
 *
 * The frame: positions are put on the map's Web Mercator plane, made
 * relative to an origin (the flight's first position) and divided by how
 * long a metre is there on that plane. So x runs east, y runs south, both
 * in about metres, and z is the altitude in metres. Multiplying by that
 * metre again and adding the origin gives the map's own coordinates, which
 * is the one transform the layer does on the graphics card.
 */

import type {
	FlightMoment,
	GlideSummary,
	GroundUnderFlight,
	ThermalSummary,
	TrackPoint
} from '#lib/core/index.ts';

/** A place in the local frame: east, south and up, in metres. */
export type Local = [x: number, y: number, z: number];

/** A colour as the graphics card takes it: red, green, blue, opacity, each 0 to 1. */
export type Rgba = [number, number, number, number];

/** The map's Web Mercator x of a longitude, 0 to 1 around the world. */
export function mercatorX(lon: number): number {
	return (180 + lon) / 360;
}

/** The map's Web Mercator y of a latitude, 0 at the top of the world and 1 at its foot. */
export function mercatorY(lat: number): number {
	return (180 - (180 / Math.PI) * Math.log(Math.tan(Math.PI / 4 + (lat * Math.PI) / 360))) / 360;
}

/** The earth's circumference at the equator as Web Mercator has it, in metres. */
const EARTH_M = 2 * Math.PI * 6_378_137;

/** How long a metre is on the Mercator plane at a latitude, in the plane's units. */
export function metreOnPlane(lat: number): number {
	return 1 / (EARTH_M * Math.cos((lat * Math.PI) / 180));
}

/** The local frame of a flight: its origin on the plane, and the length of a metre there. */
export interface Frame {
	/** The origin's place on the Mercator plane. */
	x: number;
	y: number;
	/** How long a metre is on the plane at the origin. */
	metre: number;
}

export function frameAt(lat: number, lon: number): Frame {
	return { x: mercatorX(lon), y: mercatorY(lat), metre: metreOnPlane(lat) };
}

/** A position in the local frame. */
export function toLocal(frame: Frame, lat: number, lon: number, altitudeM: number): Local {
	return [
		(mercatorX(lon) - frame.x) / frame.metre,
		(mercatorY(lat) - frame.y) / frame.metre,
		altitudeM
	];
}

/**
 * The matrix that takes the local frame to the map's Mercator coordinates,
 * column by column as WebGL reads it: scale by the metre, then move to the
 * origin. Altitude is scaled by the same metre, as the map scales terrain.
 */
export function frameMatrix(frame: Frame): Float64Array {
	const m = frame.metre;
	// prettier-ignore
	return new Float64Array([
		m, 0, 0, 0,
		0, m, 0, 0,
		0, 0, m, 0,
		frame.x, frame.y, 0, 1
	]);
}

/** The product of two 4 by 4 matrices in WebGL's order: `a` after `b`. */
export function multiply(a: ArrayLike<number>, b: ArrayLike<number>): Float32Array {
	const out = new Float32Array(16);
	for (let column = 0; column < 4; column += 1) {
		for (let row = 0; row < 4; row += 1) {
			let sum = 0;
			for (let k = 0; k < 4; k += 1) sum += a[k * 4 + row] * b[column * 4 + k];
			out[column * 4 + row] = sum;
		}
	}
	return out;
}

/** A colour written `#RRGGBB`, with an opacity. */
export function rgba(hex: string, opacity = 1): Rgba {
	const value = Number.parseInt(hex.slice(1, 7), 16);
	return [((value >> 16) & 255) / 255, ((value >> 8) & 255) / 255, (value & 255) / 255, opacity];
}

/**
 * Triangles that make a ribbon along a line: every vertex has its place on
 * the line, the way it stands out from it (a unit vector, flipped for the
 * ribbon's other edge) and a colour. The graphics card moves each vertex
 * out by the ribbon's width, which follows the zoom, so the buffers are
 * built once. Each segment is two ribbons crossed: one lying flat and one
 * standing up, so that the line is seen from above and from the side.
 */
export interface Ribbon {
	centres: Float32Array;
	sides: Float32Array;
	colours: Float32Array;
	/** How many vertices there are. */
	count: number;
}

/** Vertices for each segment: two crossed quads of two triangles each. */
const PER_SEGMENT = 12;

export function ribbon(points: readonly Local[], colours: readonly Rgba[]): Ribbon {
	const segments = Math.max(points.length - 1, 0);
	const centres = new Float32Array(segments * PER_SEGMENT * 3);
	const sides = new Float32Array(segments * PER_SEGMENT * 3);
	const colourOf = new Float32Array(segments * PER_SEGMENT * 4);
	let vertex = 0;
	const put = (point: Local, side: Local, colour: Rgba) => {
		centres.set(point, vertex * 3);
		sides.set(side, vertex * 3);
		colourOf.set(colour, vertex * 4);
		vertex += 1;
	};
	for (let index = 0; index < segments; index += 1) {
		const [a, b] = [points[index], points[index + 1]];
		const colour = colours[index + 1] ?? colours[index];
		const [dx, dy] = [b[0] - a[0], b[1] - a[1]];
		const length = Math.hypot(dx, dy);
		// Flat across the way flown; for a segment straight up, across east.
		const flat: Local = length > 1e-6 ? [-dy / length, dx / length, 0] : [1, 0, 0];
		const up: Local = [0, 0, 1];
		for (const side of [flat, up]) {
			const minus: Local = [-side[0], -side[1], -side[2]];
			put(a, side, colour);
			put(a, minus, colour);
			put(b, side, colour);
			put(b, side, colour);
			put(a, minus, colour);
			put(b, minus, colour);
		}
	}
	return { centres, sides, colours: colourOf, count: vertex };
}

/** Triangles with a place and a colour each. */
export interface Mesh {
	positions: Float32Array;
	colours: Float32Array;
	count: number;
}

/**
 * The curtain: a wall from the track straight down to the ground under it,
 * so that the eye reads how high the glider was. Where the ground under a
 * stretch is not known, the curtain has a gap there.
 */
export function curtain(
	points: readonly Local[],
	groundAt: (index: number) => number | undefined,
	colour: Rgba
): Mesh {
	const positions: number[] = [];
	for (let index = 0; index + 1 < points.length; index += 1) {
		const [ga, gb] = [groundAt(index), groundAt(index + 1)];
		if (ga === undefined || gb === undefined) continue;
		const [a, b] = [points[index], points[index + 1]];
		const [a0, b0]: Local[] = [
			[a[0], a[1], Math.min(ga, a[2])],
			[b[0], b[1], Math.min(gb, b[2])]
		];
		positions.push(...a, ...a0, ...b, ...b, ...a0, ...b0);
	}
	const count = positions.length / 3;
	const colours = new Float32Array(count * 4);
	for (let vertex = 0; vertex < count; vertex += 1) colours.set(colour, vertex * 4);
	return { positions: new Float32Array(positions), colours, count };
}

/**
 * The height of the ground at a moment of the flight, from the moments the
 * ground was read at: between two of them, along the line between their
 * heights; outside them, or across a gap wider than twice their spacing,
 * nothing.
 */
export function groundAtSeconds(
	ground: readonly GroundUnderFlight[],
	seconds: number
): number | undefined {
	if (ground.length === 0) return undefined;
	let low = 0;
	let high = ground.length - 1;
	if (seconds < ground[0].seconds || seconds > ground[high].seconds) return undefined;
	while (high - low > 1) {
		const middle = (low + high) >> 1;
		if (ground[middle].seconds <= seconds) low = middle;
		else high = middle;
	}
	const [a, b] = [ground[low], ground[high]];
	if (a === b || b.seconds === a.seconds) return a.elevationM;
	const spacing = (ground[ground.length - 1].seconds - ground[0].seconds) / (ground.length - 1);
	if (b.seconds - a.seconds > 2 * Math.max(spacing, 1)) return undefined;
	const share = (seconds - a.seconds) / (b.seconds - a.seconds);
	return a.elevationM + (b.elevationM - a.elevationM) * share;
}

/** A word written in the air beside the flight, with where. */
export interface AirLabel {
	/** Where the word stands, in the local frame. */
	at: Local;
	/** Where that is on the map, for a camera that follows. */
	lat: number;
	lon: number;
	text: string;
	kind: 'thermal' | 'sink';
}

/** The middle of the track between two moments, in the local frame, at a height of its own. */
function middleOf(
	track: readonly TrackPoint[],
	from: FlightMoment,
	to: FlightMoment
): { lat: number; lon: number } {
	const first = Math.max(0, Math.min(from.trackIndex, track.length - 1));
	const last = Math.max(first, Math.min(to.trackIndex, track.length - 1));
	let [lat, lon] = [0, 0];
	for (let index = first; index <= last; index += 1) {
		lat += track[index].lat;
		lon += track[index].lon;
	}
	const count = last - first + 1;
	return { lat: lat / count, lon: lon / count };
}

/**
 * A thermal as a column in the air: from the height it was entered at to
 * the height it was left at, above the middle of the circles flown in it,
 * with its words at the top. The numbers are the core's.
 */
export interface ThermalColumn {
	bottom: Local;
	top: Local;
	label: AirLabel;
}

export function thermalColumns(
	frame: Frame,
	track: readonly TrackPoint[],
	thermals: readonly ThermalSummary[],
	words: (thermal: ThermalSummary) => string
): ThermalColumn[] {
	return thermals.map((thermal) => {
		const { lat, lon } = middleOf(track, thermal.entry, thermal.exit);
		const bottom = toLocal(frame, lat, lon, thermal.entry.altitudeM);
		const top = toLocal(frame, lat, lon, Math.max(thermal.exit.altitudeM, thermal.entry.altitudeM));
		return { bottom, top, label: { at: top, lat, lon, text: words(thermal), kind: 'thermal' } };
	});
}

/**
 * Where the flight lost the most height in one glide, as the core's
 * analysis counts the glides: the word at the middle of that glide, at the
 * height it began at. Nothing for a flight without glides that lost height.
 */
export function deepestGlide(
	frame: Frame,
	track: readonly TrackPoint[],
	glides: readonly GlideSummary[],
	words: (glide: GlideSummary) => string
): AirLabel | undefined {
	const deepest = glides.reduce<GlideSummary | undefined>(
		(most, glide) =>
			glide.heightLostM > 0 && (!most || glide.heightLostM > most.heightLostM) ? glide : most,
		undefined
	);
	if (!deepest) return undefined;
	const { lat, lon } = middleOf(track, deepest.start, deepest.end);
	const altitude = (deepest.start.altitudeM + deepest.end.altitudeM) / 2;
	return {
		at: toLocal(frame, lat, lon, altitude),
		lat,
		lon,
		text: words(deepest),
		kind: 'sink'
	};
}

/**
 * Where a place of the local frame lies on the screen, in CSS pixels from
 * the top left, through the matrix that takes it to clip space. Nothing for
 * a place behind the camera or off the screen.
 */
export function onScreen(
	matrix: ArrayLike<number>,
	at: Local,
	width: number,
	height: number
): { x: number; y: number } | undefined {
	const [x, y, z] = at;
	const clipX = matrix[0] * x + matrix[4] * y + matrix[8] * z + matrix[12];
	const clipY = matrix[1] * x + matrix[5] * y + matrix[9] * z + matrix[13];
	const clipW = matrix[3] * x + matrix[7] * y + matrix[11] * z + matrix[15];
	if (clipW <= 0) return undefined;
	const [nx, ny] = [clipX / clipW, clipY / clipW];
	if (Math.abs(nx) > 1.1 || Math.abs(ny) > 1.1) return undefined;
	return { x: ((nx + 1) / 2) * width, y: ((1 - ny) / 2) * height };
}

/**
 * The way the glider was flying at a position, in degrees clockwise from
 * north: from the position some seconds before. For a camera that follows
 * from behind.
 */
export function headingAt(track: readonly TrackPoint[], index: number, back = 20): number {
	const here = track[Math.min(index, track.length - 1)];
	let before = index;
	while (before > 0 && here.seconds - track[before].seconds < back) before -= 1;
	const there = track[before];
	const north = here.lat - there.lat;
	const east = (here.lon - there.lon) * Math.cos((here.lat * Math.PI) / 180);
	if (Math.hypot(north, east) < 1e-9) return 0;
	return ((Math.atan2(east, north) * 180) / Math.PI + 360) % 360;
}
