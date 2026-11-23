/**
 * The glider in the flight map's 3D view, and the camera that chases it:
 * a small paraglider (a wing, an arc, over its pilot) pointed along the
 * track and banked into its turns as the track's curve says, and a camera
 * behind and above it that follows smoothly. And the replay: how far the
 * marked moment moves at the speed the pilot chose.
 *
 * Pure functions, so that they are tested on their own; `AirLayer.ts`
 * draws the wing, `FlightMap.svelte` moves the camera, `FlightScreen.svelte`
 * plays the flight. The glider is drawn at a size on the screen, not in
 * metres: a wing of ten metres would be a pixel at the zoom of a flight.
 */

import type { TrackPoint } from '#lib/core/index.ts';
import type { Local, Mesh, Rgba } from './air';

/** The wing on the screen, in pixels: its span, its chord, how far its tips hang and how high over the pilot it flies. */
export const WING_PX = { span: 44, chord: 11, sag: 9, over: 18 } as const;

/** How many pieces the wing's arc is made of from tip to tip. */
const WING_PIECES = 12;

/**
 * The wing as triangles in the glider's own frame, in pixels: x to the
 * right, y forward, z up, the pilot at the origin. An arc from tip to tip,
 * its tips lower and narrower, coloured `colour` and darker towards the
 * tips; and the two lines from the pilot up to it, in `edge`.
 */
export function wing(colour: Rgba, edge: Rgba): { canopy: Mesh; lines: Mesh } {
	const positions: number[] = [];
	const colours: number[] = [];
	const station = (s: number) => {
		const half = (WING_PX.chord / 2) * (1 - 0.35 * s * s);
		const x = (s * WING_PX.span) / 2;
		const z = WING_PX.over - WING_PX.sag * s * s;
		const shade = 0.35 * s * s;
		const tint = colour.map((value, index) =>
			index === 3 ? value : value * (1 - shade) + edge[index] * shade
		);
		return { front: [x, half, z], back: [x, -half, z], tint };
	};
	for (let piece = 0; piece < WING_PIECES; piece += 1) {
		const a = station(-1 + (2 * piece) / WING_PIECES);
		const b = station(-1 + (2 * (piece + 1)) / WING_PIECES);
		for (const [at, tint] of [
			[a.front, a.tint],
			[a.back, a.tint],
			[b.front, b.tint],
			[b.front, b.tint],
			[a.back, a.tint],
			[b.back, b.tint]
		] as const) {
			positions.push(...at);
			colours.push(...tint);
		}
	}
	const reach = station(0.6);
	const lines = [
		0,
		0,
		0,
		-reach.front[0],
		0,
		reach.front[2],
		0,
		0,
		0,
		reach.front[0],
		0,
		reach.front[2]
	];
	return {
		canopy: {
			positions: new Float32Array(positions),
			colours: new Float32Array(colours),
			count: positions.length / 3
		},
		lines: {
			positions: new Float32Array(lines),
			colours: new Float32Array([...edge, ...edge, ...edge, ...edge]),
			count: 4
		}
	};
}

/**
 * The matrix that takes the glider's own frame to the flight's local frame
 * (east, south, up, in metres), column by column as WebGL reads it: scaled
 * by `pixelM` (a pixel on the ground, in metres), banked by `bankDeg` (to
 * the right when positive), turned to `headingDeg` (clockwise from north),
 * and moved to `at`.
 */
export function gliderMatrix(
	at: Local,
	headingDeg: number,
	bankDeg: number,
	pixelM: number
): Float64Array {
	const heading = (headingDeg * Math.PI) / 180;
	const bank = (bankDeg * Math.PI) / 180;
	const forward: Local = [Math.sin(heading), -Math.cos(heading), 0];
	const right: Local = [Math.cos(heading), Math.sin(heading), 0];
	const [c, s] = [Math.cos(bank), Math.sin(bank)];
	// Banked right, the right wing goes down and the wing's up leans right.
	const banked: Local = [right[0] * c, right[1] * c, -s];
	const up: Local = [right[0] * s, right[1] * s, c];
	const k = pixelM;
	// prettier-ignore
	return new Float64Array([
		banked[0] * k, banked[1] * k, banked[2] * k, 0,
		forward[0] * k, forward[1] * k, forward[2] * k, 0,
		up[0] * k, up[1] * k, up[2] * k, 0,
		at[0], at[1], at[2], 1
	]);
}

/** The way from `from` to `to` the short way round, in degrees: −180 to 180, clockwise positive. */
export function turnBetween(from: number, to: number): number {
	return ((((to - from + 180) % 360) + 360) % 360) - 180;
}

/** The steepest bank drawn, in degrees. */
const MOST_BANK = 55;

/**
 * How far the glider banked at a position, in degrees, to the right when
 * positive: from how far it turned and how far it flew over the last
 * `window` seconds, piece by piece of the track, as a coordinated turn
 * banks (tan bank = speed × turn rate / g). Nothing on a straight line.
 */
export function bankAt(track: readonly TrackPoint[], index: number, window = 6): number {
	const i = Math.max(0, Math.min(index, track.length - 1));
	let j = i;
	while (j > 0 && track[i].seconds - track[j].seconds < window) j -= 1;
	const seconds = track[i].seconds - track[j].seconds;
	if (seconds <= 0) return 0;
	let [turned, flownM] = [0, 0];
	let before: number | undefined;
	/** The middle moments of the first piece and the last: the turning is between them. */
	let [firstMiddle, lastMiddle] = [0, 0];
	for (let k = j + 1; k <= i; k += 1) {
		const [a, b] = [track[k - 1], track[k]];
		const northM = (b.lat - a.lat) * 111_320;
		const eastM = (b.lon - a.lon) * 111_320 * Math.cos((b.lat * Math.PI) / 180);
		const lengthM = Math.hypot(northM, eastM);
		// A position that did not move says nothing of the way flown.
		if (lengthM < 0.5) continue;
		const heading = (Math.atan2(eastM, northM) * 180) / Math.PI;
		const middle = (a.seconds + b.seconds) / 2;
		if (before === undefined) firstMiddle = middle;
		else turned += turnBetween(before, heading);
		before = heading;
		lastMiddle = middle;
		flownM += lengthM;
	}
	if (lastMiddle <= firstMiddle) return 0;
	const rate = (turned * Math.PI) / 180 / (lastMiddle - firstMiddle);
	const bank = (Math.atan(((flownM / seconds) * rate) / 9.81) * 180) / Math.PI;
	return Math.max(-MOST_BANK, Math.min(MOST_BANK, bank));
}

/** Where the chasing camera looks, and from which way. */
export interface Chase {
	lat: number;
	lon: number;
	/** The height of the point looked at, the glider's, in metres. */
	altitudeM: number;
	/** The way the camera looks, clockwise from north: the glider's heading, behind it. */
	bearing: number;
}

/** How quickly the camera catches up, in seconds: soon with the glider's place, slowly with its turns. */
const PLACE_S = 0.2;
const TURN_S = 1.4;

/**
 * One step of the chasing camera towards the glider after `seconds`: an
 * easing that keeps up with where the glider is and swings round its
 * turns slowly, so that circling in a thermal does not spin the view.
 */
export function chaseStep(from: Chase, to: Chase, seconds: number): Chase {
	const place = 1 - Math.exp(-seconds / PLACE_S);
	const turn = 1 - Math.exp(-seconds / TURN_S);
	return {
		lat: from.lat + (to.lat - from.lat) * place,
		lon: from.lon + (to.lon - from.lon) * place,
		altitudeM: from.altitudeM + (to.altitudeM - from.altitudeM) * place,
		bearing: (from.bearing + turnBetween(from.bearing, to.bearing) * turn + 360) % 360
	};
}

/** Whether the camera has as good as caught up: nothing left to move for. */
export function caughtUp(from: Chase, to: Chase): boolean {
	return (
		Math.abs(from.lat - to.lat) < 1e-7 &&
		Math.abs(from.lon - to.lon) < 1e-7 &&
		Math.abs(from.altitudeM - to.altitudeM) < 0.05 &&
		Math.abs(turnBetween(from.bearing, to.bearing)) < 0.05
	);
}

/** The replay's speeds, times the flight's own time, and the one it starts at. */
export const REPLAY_SPEEDS = [1, 10, 30, 60] as const;
export type ReplaySpeed = (typeof REPLAY_SPEEDS)[number];
export const FIRST_SPEED: ReplaySpeed = 30;

/** A speed in words, as the choice shows it: "30×". */
export function speedWords(speed: number): string {
	return `${speed}×`;
}

/**
 * Where the replay is after `seconds` of the pilot's time at `speed`: the
 * flight's time it has come to (`playhead`, which moves smoothly however
 * far apart the positions were recorded) and the last position at or
 * before it. A replay starts at the position it is at.
 */
export function replayed(
	track: readonly TrackPoint[],
	playhead: number,
	seconds: number,
	speed: number
): { playhead: number; index: number } {
	const last = track.length - 1;
	const end = track[last]?.seconds ?? 0;
	const at = Math.min(playhead + seconds * speed, end);
	let [low, high] = [0, last];
	while (low < high) {
		const middle = (low + high + 1) >> 1;
		if (track[middle].seconds <= at) low = middle;
		else high = middle - 1;
	}
	return { playhead: at, index: Math.max(low, 0) };
}
