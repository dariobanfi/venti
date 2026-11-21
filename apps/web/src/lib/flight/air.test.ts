import { describe, expect, it } from 'vitest';
import type { FlightMoment, GlideSummary, ThermalSummary, TrackPoint } from '#lib/core/index.ts';
import {
	curtain,
	deepestGlide,
	frameAt,
	frameMatrix,
	groundAtSeconds,
	headingAt,
	mercatorX,
	mercatorY,
	metreOnPlane,
	multiply,
	onScreen,
	ribbon,
	rgba,
	thermalColumns,
	toLocal
} from './air';

const point = (seconds: number, lat: number, lon: number, altitudeM: number): TrackPoint => ({
	seconds,
	lat,
	lon,
	altitudeM,
	climbMs: 0
});
const moment = (trackIndex: number, altitudeM: number): FlightMoment => ({
	trackIndex,
	seconds: trackIndex,
	position: { lat: 0, lon: 0 },
	altitudeM
});

describe('a flight in the air', () => {
	const frame = frameAt(47.66, 11.8);

	it('is placed in metres east, south and up of its first position', () => {
		expect(toLocal(frame, 47.66, 11.8, 1500)).toEqual([0, 0, 1500]);
		const east = toLocal(frame, 47.66, 11.8 + 0.01, 1500);
		// A hundredth of a degree of longitude at 47.66° is about 750 m.
		expect(east[0]).toBeCloseTo(750, -1);
		expect(east[1]).toBeCloseTo(0, 6);
		const north = toLocal(frame, 47.67, 11.8, 1500);
		expect(north[1]).toBeLessThan(0);
		expect(-north[1]).toBeCloseTo(1112, -1);
	});

	it('goes back to the map’s Mercator coordinates through one matrix, altitude scaled like terrain', () => {
		const at = toLocal(frame, 47.7, 11.9, 2000);
		const m = frameMatrix(frame);
		const x = m[0] * at[0] + m[12];
		const y = m[5] * at[1] + m[13];
		const z = m[10] * at[2];
		expect(x).toBeCloseTo(mercatorX(11.9), 12);
		expect(y).toBeCloseTo(mercatorY(47.7), 12);
		expect(z).toBeCloseTo(2000 * metreOnPlane(47.66), 15);
		const identity = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1];
		expect(Array.from(multiply(identity, m))).toEqual(Array.from(new Float32Array(m)));
	});

	it('is a crossed ribbon: twelve vertices a segment, flat across the way flown and standing up', () => {
		const points: [number, number, number][] = [
			[0, 0, 1000],
			[100, 0, 1010],
			[100, 100, 1020]
		];
		const made = ribbon(points, [rgba('#ff0000'), rgba('#00ff00'), rgba('#0000ff')]);
		expect(made.count).toBe(24);
		// The first segment runs east: flat across it is north or south.
		expect(Array.from(made.sides.slice(0, 3))).toEqual([-0, 1, 0]);
		// Its second quad stands up.
		expect(Array.from(made.sides.slice(18, 21))).toEqual([0, 0, 1]);
		// A segment takes the colour of where it ends.
		expect(Array.from(made.colours.slice(0, 4))).toEqual([0, 1, 0, 1]);
		expect(ribbon([[0, 0, 0]], [rgba('#000000')]).count).toBe(0);
	});

	it('hangs a curtain down to the ground, with a gap where the ground is not known', () => {
		const points: [number, number, number][] = [
			[0, 0, 1000],
			[10, 0, 1000],
			[20, 0, 1000]
		];
		const wall = curtain(points, (index) => (index === 2 ? undefined : 700), rgba('#302157', 0.16));
		expect(wall.count).toBe(6);
		expect(Array.from(wall.positions.slice(3, 6))).toEqual([0, 0, 700]);
		expect(wall.colours[3]).toBeCloseTo(0.16, 6);
	});

	it('reads the ground between the moments it was read at, and not across a gap', () => {
		const ground = [0, 10, 20, 30, 100].map((seconds) => ({
			seconds,
			elevationM: 1000 + seconds,
			heightAboveM: 0
		}));
		expect(groundAtSeconds(ground, 5)).toBe(1005);
		expect(groundAtSeconds(ground, 20)).toBe(1020);
		expect(groundAtSeconds(ground, 60)).toBeUndefined();
		expect(groundAtSeconds(ground, -1)).toBeUndefined();
		expect(groundAtSeconds([], 5)).toBeUndefined();
	});

	const track = [0, 1, 2, 3, 4].map((index) => point(index, 47.66 + index * 0.001, 11.8, 1000));

	it('raises a thermal as a column over the middle of its circles, from entry to exit height', () => {
		const thermal: ThermalSummary = {
			entry: moment(1, 1100),
			exit: moment(3, 1450),
			gainM: 350,
			climbMs: 2.4,
			wind: undefined
		};
		const [column] = thermalColumns(frame, track, [thermal], () => '+2.4 m/s · +350 m');
		expect(column.bottom[2]).toBe(1100);
		expect(column.top[2]).toBe(1450);
		expect(column.label.lat).toBeCloseTo(47.662, 9);
		expect(column.label.text).toBe('+2.4 m/s · +350 m');
	});

	it('marks the glide that lost the most height, and none when none lost any', () => {
		const glide = (from: number, to: number, lost: number): GlideSummary => ({
			start: moment(from, 1500),
			end: moment(to, 1500 - lost),
			distanceM: 1000,
			heightLostM: lost,
			glideRatio: 8,
			speedMs: 10
		});
		const deepest = deepestGlide(frame, track, [glide(0, 1, 100), glide(2, 4, 400)], (g) =>
			String(g.heightLostM)
		);
		expect(deepest?.text).toBe('400');
		expect(deepest?.at[2]).toBe(1300);
		expect(deepestGlide(frame, track, [glide(0, 1, -5)], () => '')).toBeUndefined();
	});

	it('is placed on the screen through the matrix to clip space, and not when behind the camera', () => {
		const identity = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1];
		expect(onScreen(identity, [0, 0, 0], 200, 100)).toEqual({ x: 100, y: 50 });
		expect(onScreen(identity, [1, 1, 0], 200, 100)).toEqual({ x: 200, y: 0 });
		const behind = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, -1];
		expect(onScreen(behind, [0, 0, 0], 200, 100)).toBeUndefined();
	});

	it('knows which way the glider flew, for a camera from behind', () => {
		expect(headingAt(track, 4, 3)).toBeCloseTo(0, 6);
		const east = [0, 1, 2].map((index) => point(index * 10, 47, 11 + index * 0.001, 1000));
		expect(headingAt(east, 2)).toBeCloseTo(90, 6);
	});
});
