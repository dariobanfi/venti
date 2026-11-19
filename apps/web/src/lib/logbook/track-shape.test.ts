import { describe, expect, it } from 'vitest';
import { outlinePoints } from './track-shape';

function points(text: string | undefined): [number, number][] {
	return (text ?? '').split(' ').map((pair) => pair.split(',').map(Number) as [number, number]);
}

describe('a track thumbnail', () => {
	it('has north at the top and east at the right', () => {
		const shape = points(
			outlinePoints(
				[
					{ lat: 0, lon: 0 },
					{ lat: 1, lon: 1 }
				],
				52,
				52,
				8
			)
		);

		// From the south-west corner to the north-east one.
		expect(shape).toEqual([
			[8, 44],
			[44, 8]
		]);
	});

	it('keeps the track’s proportions and centres it', () => {
		// Twice as far north as east, at the equator.
		const shape = points(
			outlinePoints(
				[
					{ lat: 0, lon: 0 },
					{ lat: 2, lon: 1 }
				],
				52,
				52,
				8
			)
		);

		expect(shape[0]).toEqual([17, 44]);
		expect(shape[1]).toEqual([35, 8]);
	});

	it('draws a degree of longitude narrower the further from the equator', () => {
		// At 60 degrees north a degree of longitude is half as wide.
		const shape = points(
			outlinePoints(
				[
					{ lat: 60, lon: 10 },
					{ lat: 61, lon: 12 }
				],
				52,
				52,
				8
			)
		);

		const width = shape[1][0] - shape[0][0];
		const height = shape[0][1] - shape[1][1];
		expect(width).toBeCloseTo(height, 0);
	});

	it('has no shape for fewer than two points', () => {
		expect(outlinePoints([], 52, 52, 8)).toBeUndefined();
		expect(outlinePoints([{ lat: 47, lon: 11 }], 52, 52, 8)).toBeUndefined();
	});

	it('draws a track with no extent as a dot in the middle, not at infinity', () => {
		const shape = points(
			outlinePoints(
				[
					{ lat: 47, lon: 11 },
					{ lat: 47, lon: 11 }
				],
				52,
				52,
				8
			)
		);
		expect(shape).toEqual([
			[26, 26],
			[26, 26]
		]);
	});
});
