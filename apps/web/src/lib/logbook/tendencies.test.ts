import { describe, expect, it } from 'vitest';
import { barPercent } from './tendencies';

describe('the bar of a share', () => {
	it('fills as much of its track as the core’s share, to a tenth of a percent', () => {
		expect(barPercent(0.23509)).toBe(23.5);
		expect(barPercent(0)).toBe(0);
		expect(barPercent(1)).toBe(100);
	});

	it('never leaves its track', () => {
		expect(barPercent(-0.2)).toBe(0);
		expect(barPercent(1.4)).toBe(100);
		expect(barPercent(Number.NaN)).toBe(0);
	});
});
