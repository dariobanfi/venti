import { describe, expect, it } from 'vitest';
import { LARGEST_SENT, LONGEST_SIDE, shrunkSize } from './picture';

describe('the size a picture is sent at', () => {
	it('leaves a small picture as it is', () => {
		expect(shrunkSize(800, 600, 300_000)).toBeUndefined();
		expect(shrunkSize(LONGEST_SIDE, LONGEST_SIDE, LARGEST_SENT)).toBeUndefined();
	});

	it('draws a photograph from a phone at the longest side, keeping its shape', () => {
		expect(shrunkSize(4032, 3024, 3_500_000)).toEqual({ width: LONGEST_SIDE, height: 768 });
		expect(shrunkSize(3024, 4032, 3_500_000)).toEqual({ width: 768, height: LONGEST_SIDE });
	});

	it('draws a picture of few pixels but many bytes anew at its own size', () => {
		expect(shrunkSize(900, 900, LARGEST_SENT + 1)).toEqual({ width: 900, height: 900 });
	});
});
