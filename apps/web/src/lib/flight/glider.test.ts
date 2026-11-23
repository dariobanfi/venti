import { describe, expect, it } from 'vitest';
import type { TrackPoint } from '#lib/core/index.ts';
import {
	bankAt,
	caughtUp,
	type Chase,
	chaseStep,
	gliderMatrix,
	replayed,
	speedWords,
	turnBetween,
	wing,
	WING_PX
} from './glider';

const point = (seconds: number, lat: number, lon: number): TrackPoint => ({
	seconds,
	lat,
	lon,
	altitudeM: 1500,
	climbMs: 0
});

/** A circle flown clockwise (to the right), at 10 m/s unless said, one position a second. */
function circling(radiusM: number, speedMs = 10, seconds = 60): TrackPoint[] {
	const metreLat = 1 / 111_320;
	const metreLon = metreLat / Math.cos((47 * Math.PI) / 180);
	const rate = speedMs / radiusM;
	return Array.from({ length: seconds }, (_, t) =>
		point(
			t,
			47 + Math.cos(t * rate) * radiusM * metreLat,
			11 + Math.sin(t * rate) * radiusM * metreLon
		)
	);
}

/** A straight line to the east, at about 10 m/s. */
const straight = Array.from({ length: 30 }, (_, t) => point(t, 47, 11 + (t * 10) / 75_900));

/** Where the matrix takes a point of the glider's own frame. */
function apply(m: Float64Array, [x, y, z]: [number, number, number]) {
	return [0, 1, 2].map((row) => m[row] * x + m[4 + row] * y + m[8 + row] * z + m[12 + row]);
}

describe('the glider in the air', () => {
	it('is a wing over its pilot: an arc whose tips hang lower, and two lines up to it', () => {
		const ink = [0, 0, 0, 1] as const;
		const { canopy, lines } = wing([1, 0.5, 0.4, 1], [...ink]);
		expect(canopy.count).toBe(12 * 6);
		const heights = Array.from({ length: canopy.count }, (_, v) => canopy.positions[v * 3 + 2]);
		expect(Math.max(...heights)).toBeCloseTo(WING_PX.over, 6);
		expect(Math.min(...heights)).toBeCloseTo(WING_PX.over - WING_PX.sag, 6);
		const spans = Array.from({ length: canopy.count }, (_, v) => canopy.positions[v * 3]);
		expect(Math.max(...spans) - Math.min(...spans)).toBeCloseTo(WING_PX.span, 6);
		expect(lines.count).toBe(4);
		// The lines start at the pilot.
		expect(Array.from(lines.positions.slice(0, 3))).toEqual([0, 0, 0]);
	});

	it('points along the track and banks into a turn, at a size on the screen', () => {
		// Flying east, level: forward is east, right is south, a pixel is 5 m.
		const east = gliderMatrix([100, 200, 1500], 90, 0, 5);
		expect(apply(east, [0, 0, 0])).toEqual([100, 200, 1500]);
		const ahead = apply(east, [0, 1, 0]);
		expect(ahead[0]).toBeCloseTo(105, 9);
		expect(ahead[1]).toBeCloseTo(200, 9);
		const right = apply(east, [1, 0, 0]);
		expect(right[1]).toBeCloseTo(205, 9);
		expect(right[2]).toBeCloseTo(1500, 9);

		// Banked 30° to the right: the right tip goes down, the wing leans right.
		const banked = gliderMatrix([0, 0, 0], 0, 30, 1);
		const tip = apply(banked, [1, 0, 0]);
		expect(tip[2]).toBeCloseTo(-0.5, 9);
		const up = apply(banked, [0, 0, 1]);
		expect(up[0]).toBeCloseTo(0.5, 9);
		expect(up[2]).toBeCloseTo(Math.cos(Math.PI / 6), 9);
	});

	it('banks from the track’s curve: right in a turn to the right, steeper in a tighter one, level on a line', () => {
		expect(bankAt(straight, 20)).toBeCloseTo(0, 3);
		const wide = bankAt(circling(80), 30);
		const tight = bankAt(circling(30), 30);
		expect(wide).toBeGreaterThan(0);
		expect(tight).toBeGreaterThan(wide);
		// A coordinated turn at 10 m/s on 30 m: atan(100 / 30 / 9.81), about 19°.
		expect(tight).toBeCloseTo(18.8, 0);
		// To the left, the other way; never steeper than the steepest drawn.
		const left = circling(30).map((p) => ({ ...p, lon: 22 - p.lon }));
		expect(bankAt(left, 30)).toBeCloseTo(-tight, 3);
		// 20 m/s on 20 m would be 64°.
		expect(bankAt(circling(20, 20), 30)).toBe(55);
	});

	it('turns the short way round', () => {
		expect(turnBetween(350, 10)).toBeCloseTo(20, 9);
		expect(turnBetween(10, 350)).toBeCloseTo(-20, 9);
		expect(turnBetween(90, 90)).toBe(0);
	});
});

describe('the camera that chases the glider', () => {
	const glider: Chase = { lat: 47.01, lon: 11.01, altitudeM: 2000, bearing: 10 };
	const camera: Chase = { lat: 47, lon: 11, altitudeM: 1500, bearing: 330 };

	it('keeps up with the glider’s place soon and swings round its turns slowly', () => {
		const step = chaseStep(camera, glider, 0.2);
		// After its time to catch up, about two thirds of the way.
		expect((step.lat - camera.lat) / (glider.lat - camera.lat)).toBeCloseTo(1 - Math.exp(-1), 6);
		const turned = turnBetween(camera.bearing, step.bearing);
		expect(turned).toBeGreaterThan(0);
		expect(turned).toBeLessThan(10);
	});

	it('comes to rest on the glider, the short way round', () => {
		let at = camera;
		for (let frame = 0; frame < 600; frame += 1) at = chaseStep(at, glider, 1 / 60);
		expect(caughtUp(at, glider)).toBe(true);
		expect(caughtUp(camera, glider)).toBe(false);
		// Never more than the short way: from 330° through north to 10°.
		const once = chaseStep(camera, glider, 1);
		expect(once.bearing > 330 || once.bearing < 10).toBe(true);
	});
});

describe('the replay', () => {
	const track = [point(0, 47, 11), point(4, 47, 11), point(5, 47, 11), point(20, 47, 11)];

	it('moves the flight’s time at the speed chosen, to the last position before it', () => {
		expect(replayed(track, 0, 0.1, 30)).toEqual({ playhead: 3, index: 0 });
		expect(replayed(track, 3, 0.1, 30)).toEqual({ playhead: 6, index: 2 });
		expect(replayed(track, 0, 1, 1)).toEqual({ playhead: 1, index: 0 });
		// Never past the end.
		expect(replayed(track, 6, 10, 60)).toEqual({ playhead: 20, index: 3 });
	});

	it('names its speeds as the choice shows them', () => {
		expect(speedWords(60)).toBe('60×');
	});
});
